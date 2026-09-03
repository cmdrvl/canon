#![forbid(unsafe_code)]

//! Deterministic, offline Canon Geo planning.
//!
//! The artifact defined here is a semantic overlay over exactly one
//! `canon.project.plan.v1` DAG.  It never executes a leaf command or reaches a
//! catalog.  Missing local evidence becomes a typed external request or an
//! explicit discovery gap.

use super::executor::{GEO_PROPAGATE_OUTPUT_ID, GEO_PROPAGATE_STAGE_COMMAND};
use super::satisfy::{
    CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION, GeoInventoryAdvancementEffect,
    GeoRegionalInventoryAdvancement, GeoRegionalInventorySourceAdvancement, GeoSatisfyError,
    geo_regional_inventory_advancement_semantic_hash, validate_geo_regional_inventory_advancement,
};
use super::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_REQUEST_VERSION,
    CANON_GEO_CAPABILITIES_VERSION, CANON_GEO_COMPOSITION_VERSION,
    CANON_GEO_DISCOVERY_REQUEST_VERSION, CANON_GEO_EVIDENCE_COMPILATION_VERSION,
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
    CANON_GEO_PROPAGATION_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION, GeoAcquisitionRequest,
    GeoBoundedSubset, GeoCapabilities, GeoColumnReadabilityProbe, GeoCompositionProfile,
    GeoControlEntityLevel, GeoDigest, GeoDigestAlgorithm, GeoDiscoveryGap,
    GeoDiscoveryReleaseSelectionPolicy, GeoDiscoveryRequest, GeoDiscoveryStep, GeoEntityLevel,
    GeoEvidenceClass, GeoFieldRole, GeoInventorySupportStatus, GeoNativeEntityScope,
    GeoNumericBound, GeoOrderDirection, GeoOrderingTerm, GeoPaginationRequest,
    GeoProjectionOperation, GeoQuestion, GeoRegionalInventory, GeoRegionalSourceInstance,
    GeoReleaseSelectionMode, GeoRequestedField, GeoResourceBudget, GeoResourceCounter,
    GeoRowByteCeilings, GeoSubsetPredicate, GeoSubsetPredicateKind, GeoTelemetrySemanticEffect,
    canonicalize_capabilities, canonicalize_geo_acquisition_request,
    canonicalize_geo_discovery_request, canonicalize_question, canonicalize_regional_inventory,
    canonicalize_resource_budget, capabilities_semantic_hash, evaluate_inventory_support,
    geo_acquisition_request_id, geo_acquisition_request_semantic_hash, geo_discovery_request_id,
    question_semantic_hash, regional_inventory_planning_hash, regional_inventory_semantic_hash,
    resource_budget_semantic_hash, validate_composition_profile, validate_geo_acquisition_request,
    validate_geo_discovery_request,
};
use crate::project::{
    ProjectExtensionDagNode, ProjectExtensionDagOutput, ProjectExtensionDagRequest, ProjectPlan,
    ProjectPlanErrorCode, ProjectPlanHashRef, ProjectPlanNodeClass, ProjectPlanNodeKind,
    ProjectPlanOutputMaterialization, ProjectPlanRefusalCondition, ProjectPlanSideEffect,
    ProjectPlanSideEffectKind, compile_extension_project_plan, validate_project_plan,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_PLAN_VERSION: &str = "canon_geo_plan.v0";

const CAPABILITIES_COMMAND: &str = "canon geo capabilities --emit json";
const PLAN_COMMAND: &str = "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>";
const HOME_CELLS_COMMAND: &str = "canon geo materialize-home-cells --rows <ROWS.json>";
const TILE_WORK_COMMAND: &str = "canon geo tile-work --request <REQUEST.json>";
const MATERIALIZE_EVIDENCE_COMMAND: &str = "canon geo materialize-evidence --rows <ROWS.json>";
const COMPILE_EVIDENCE_COMMAND: &str = "canon geo compile-evidence --request <REQUEST.json>";
const PROPAGATE_COMMAND: &str = GEO_PROPAGATE_STAGE_COMMAND;
const SOLVE_COMMAND: &str = "canon geo solve --request <REQUEST.json>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoPlanRequest {
    pub question: GeoQuestion,
    pub capabilities: GeoCapabilities,
    pub inventory: GeoRegionalInventory,
    pub profile: GeoCompositionProfile,
    pub budget: GeoResourceBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoPlanReplanRequest {
    pub base_plan: GeoPlan,
    pub base_inventory: GeoRegionalInventory,
    pub question: GeoQuestion,
    pub capabilities: GeoCapabilities,
    pub profile: GeoCompositionProfile,
    pub budget: GeoResourceBudget,
    pub inventory_advancement: GeoRegionalInventoryAdvancement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPlanStatus {
    Planned,
    Partial,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPlanGrainStatus {
    PlannedRelativeToDeclaredUniverse,
    WaitingForAcquisition,
    UnsupportedByProfile,
    UnsupportedByInventory,
    MissingLeafCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPlanStage {
    MaterializeHomeCells,
    BuildBoundedSection,
    MaterializeEvidence,
    CompileEvidence,
    PropagateConstraints,
    FactorAndSolveExactResidual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPlanGatePlane {
    Availability,
    Coverage,
    CandidateReach,
    Admission,
    ConstraintEffect,
    SolverCorrectness,
    Cost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPlanGateStatus {
    SatisfiedByDeclaredInput,
    PendingArtifact,
    PassedAgainstReference,
    FailedAgainstReference,
    StructurallyCompleteRelativeToInputs,
    UnverifiedWithClaimLimitation,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPlanClaimEffect {
    CanChangeRequestedClaim,
    BlocksRequestedClaim,
    NamedAuditGate,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanArtifactRef {
    pub artifact_id: String,
    pub semantic_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanInventoryRef {
    pub inventory_id: String,
    pub semantic_hash: String,
    pub planning_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanProfileRef {
    pub version: String,
    pub selection_level: GeoEntityLevel,
    pub semantic_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanBudgetRef {
    pub budget_id: String,
    /// Hash of the complete budget artifact, including telemetry declarations.
    pub semantic_hash: String,
    /// Hash of deterministic bounds only. This controls planning semantics.
    pub planning_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanPrecondition {
    pub plane: GeoPlanGatePlane,
    pub status: GeoPlanGateStatus,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanTransitionSet {
    pub success: String,
    pub abstention: String,
    pub contradiction: String,
    pub budget_fallback: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanProducedArtifactRef {
    pub producer_node_id: String,
    pub output_id: String,
    pub output_contract: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPlanComponentScope {
    /// The shipped `canon geo solve` leaf constructs the actual variable/constraint
    /// incidence graph, factors it, and chooses a backend per connected component.
    /// Planning must not invent component ids before that typed leaf runs.
    ActualConnectedComponentsOfCompiledConstraintIncidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanExactSolveScope {
    pub bounded_section: GeoPlanProducedArtifactRef,
    pub evidence_compilation: GeoPlanProducedArtifactRef,
    pub component_scope: GeoPlanComponentScope,
    pub component_key_field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanCostEstimateRange {
    pub semantic_id: String,
    pub counter: GeoResourceCounter,
    pub lower_bound: u64,
    pub upper_bound: u64,
    pub unit: String,
    pub basis: String,
    pub semantic_effect: GeoTelemetrySemanticEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanNodeOverlay {
    pub project_node_id: String,
    pub stage: GeoPlanStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_level: Option<GeoControlEntityLevel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_classes: Vec<GeoEvidenceClass>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claim_classes: Vec<super::GeoClaimClass>,
    pub expected_output_contract: String,
    pub preconditions: Vec<GeoPlanPrecondition>,
    pub claim_effect: GeoPlanClaimEffect,
    pub bounded_section_required: bool,
    pub incidence_factorization_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_solve_scope: Option<GeoPlanExactSolveScope>,
    pub deterministic_bounds: Vec<GeoNumericBound>,
    pub cost_estimate_ranges: Vec<GeoPlanCostEstimateRange>,
    pub transitions: GeoPlanTransitionSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanGrainOutcome {
    pub entity_level: GeoControlEntityLevel,
    pub status: GeoPlanGrainStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_evidence_classes: Vec<GeoEvidenceClass>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_node_ids: Vec<String>,
    pub claim_limitation: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanAcquisitionHandoff {
    pub expected_receipt_contract: String,
    pub required_result_digest_algorithm: GeoDigestAlgorithm,
    pub continuation_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeoPlanExternalRequest {
    Acquisition {
        request: GeoAcquisitionRequest,
        handoff: GeoPlanAcquisitionHandoff,
    },
    Discovery {
        gap_id: String,
        request: GeoDiscoveryRequest,
    },
    DiscoveryGap {
        gap: GeoDiscoveryGap,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlan {
    pub version: String,
    pub plan_id: String,
    pub semantic_hash: String,
    pub status: GeoPlanStatus,
    pub question_ref: GeoPlanArtifactRef,
    pub capabilities_ref: GeoPlanArtifactRef,
    pub inventory_ref: GeoPlanInventoryRef,
    pub profile_ref: GeoPlanProfileRef,
    pub budget_ref: GeoPlanBudgetRef,
    pub project_plan: ProjectPlan,
    pub geo_nodes: Vec<GeoPlanNodeOverlay>,
    pub grain_outcomes: Vec<GeoPlanGrainOutcome>,
    pub external_requests: Vec<GeoPlanExternalRequest>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPlanErrorCode {
    InvalidInput,
    UnsupportedVersion,
    MissingCapability,
    ContractViolation,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPlanError {
    pub code: GeoPlanErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoPlanError {
    fn new(
        code: GeoPlanErrorCode,
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

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(
            GeoPlanErrorCode::InvalidInput,
            message,
            BTreeMap::<String, String>::new(),
        )
    }
}

impl fmt::Display for GeoPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoPlanError {}

#[derive(Serialize)]
struct GeoPlanSemanticProjection<'a> {
    version: &'a str,
    question_hash: &'a str,
    capabilities_hash: &'a str,
    inventory_planning_hash: &'a str,
    profile_hash: &'a str,
    budget_planning_hash: &'a str,
    project_graph_hash: &'a str,
    geo_nodes: Vec<GeoPlanNodeSemanticProjection<'a>>,
    grain_outcomes: Vec<GeoPlanGrainOutcomeSemanticProjection<'a>>,
    external_request_planning_hashes: Vec<String>,
}

#[derive(Serialize)]
struct GeoPlanPreconditionSemanticProjection {
    plane: GeoPlanGatePlane,
    status: GeoPlanGateStatus,
}

#[derive(Serialize)]
struct GeoPlanNodeSemanticProjection<'a> {
    project_node_id: &'a str,
    stage: GeoPlanStage,
    entity_level: Option<GeoControlEntityLevel>,
    evidence_classes: &'a [GeoEvidenceClass],
    claim_classes: &'a [super::GeoClaimClass],
    expected_output_contract: &'a str,
    preconditions: Vec<GeoPlanPreconditionSemanticProjection>,
    claim_effect: GeoPlanClaimEffect,
    bounded_section_required: bool,
    incidence_factorization_required: bool,
    exact_solve_scope: &'a Option<GeoPlanExactSolveScope>,
    deterministic_bounds: &'a [GeoNumericBound],
}

#[derive(Serialize)]
struct GeoPlanGrainOutcomeSemanticProjection<'a> {
    entity_level: GeoControlEntityLevel,
    status: GeoPlanGrainStatus,
    missing_evidence_classes: &'a [GeoEvidenceClass],
    project_node_ids: &'a [String],
}

#[derive(Serialize)]
struct GeoAcquisitionReleasePlanningRef<'a> {
    release_id: &'a str,
    release_digest: &'a GeoDigest,
}

#[derive(Serialize)]
struct GeoAcquisitionPlanningProjection<'a> {
    version: &'a str,
    bounded_geography: &'a super::GeoBoundedGeography,
    subset: &'a GeoBoundedSubset,
    releases: Vec<GeoAcquisitionReleasePlanningRef<'a>>,
    fields: &'a [GeoRequestedField],
    projection: &'a Option<GeoProjectionOperation>,
    ordering: &'a [GeoOrderingTerm],
    pagination: &'a GeoPaginationRequest,
    ceilings: &'a GeoRowByteCeilings,
    positive_path_min_rows: u64,
    expected_receipt_contract: &'a str,
    required_result_digest_algorithm: GeoDigestAlgorithm,
}

#[derive(Serialize)]
struct GeoDiscoveryPlanningProjection<'a> {
    version: &'a str,
    bounded_geography: &'a super::GeoBoundedGeography,
    subset: &'a GeoBoundedSubset,
    requested_entity_levels: &'a [GeoControlEntityLevel],
    requested_evidence_classes: &'a [GeoEvidenceClass],
    release_selection: &'a GeoDiscoveryReleaseSelectionPolicy,
    releases: Vec<GeoAcquisitionReleasePlanningRef<'a>>,
    fields: &'a [GeoRequestedField],
    required_steps: &'a [GeoDiscoveryStep],
    readability_fields: &'a [String],
    readability_subset: &'a GeoBoundedSubset,
    readability_ceilings: &'a GeoRowByteCeilings,
    ceilings: &'a GeoRowByteCeilings,
}

pub fn compile_geo_plan(request: GeoPlanRequest) -> Result<GeoPlan, GeoPlanError> {
    let question = canonicalize_question(&request.question).map_err(control_error)?;
    let capabilities = canonicalize_capabilities(&request.capabilities).map_err(control_error)?;
    let inventory = canonicalize_regional_inventory(&request.inventory).map_err(control_error)?;
    let budget = canonicalize_resource_budget(&request.budget).map_err(control_error)?;
    let profile = validate_profile(&request.profile)?;

    let question_hash = question_semantic_hash(&question).map_err(control_error)?;
    let capabilities_hash = capabilities_semantic_hash(&capabilities).map_err(control_error)?;
    let inventory_hash = regional_inventory_semantic_hash(&inventory).map_err(control_error)?;
    let inventory_planning_hash =
        regional_inventory_planning_hash(&inventory).map_err(control_error)?;
    let budget_hash = resource_budget_semantic_hash(&budget).map_err(control_error)?;
    let budget_planning_hash = deterministic_budget_planning_hash(&budget)?;
    let profile_hash = digest_json(&profile)?;
    let support =
        evaluate_inventory_support(&question, &inventory, &budget).map_err(control_error)?;

    require_implemented_command(
        &capabilities,
        CAPABILITIES_COMMAND,
        CANON_GEO_CAPABILITIES_VERSION,
    )?;
    require_implemented_command(&capabilities, PLAN_COMMAND, CANON_GEO_PLAN_VERSION)?;

    let limits = project_limits(&budget);
    let input_refs = vec![
        hash_ref("geo.question", &question_hash),
        hash_ref("geo.capabilities", &capabilities_hash),
        hash_ref("geo.inventory.planning", &inventory_planning_hash),
        hash_ref("geo.profile", &profile_hash),
        hash_ref("geo.budget.planning", &budget_planning_hash),
    ];
    let mut project_nodes = Vec::new();
    let mut geo_nodes = Vec::new();
    let mut outcomes = Vec::new();
    let mut external_requests = Vec::new();
    let mut diagnostics = Vec::new();
    let stable_identity_requested = question
        .requested_claim_classes
        .binary_search(&super::GeoClaimClass::StableIdentity)
        .is_ok();

    for grain in &question.requested_grains {
        let support_row = support
            .grain_support
            .iter()
            .find(|row| row.entity_level == grain.entity_level)
            .expect("support report covers every requested grain");
        let selected_level = profile_control_level(&profile);
        if selected_level != Some(grain.entity_level) {
            outcomes.push(GeoPlanGrainOutcome {
                entity_level: grain.entity_level,
                status: GeoPlanGrainStatus::UnsupportedByProfile,
                missing_evidence_classes: Vec::new(),
                project_node_ids: Vec::new(),
                claim_limitation: "the supplied composition profile selects a different entity grain; supported grains are preserved independently".to_string(),
                next_action: "supply a compatible canon_geo_composition_profile.v0 or retain this grain as unsupported".to_string(),
            });
            continue;
        }

        if support_row.status == GeoInventorySupportStatus::Unsupported {
            let mut actionable_request_count = 0_usize;
            let mut non_overwriting_repair_required = false;
            for evidence_class in &support_row.missing_evidence_classes {
                if let Some(source) = acquisition_source(
                    &inventory,
                    grain.entity_level,
                    *evidence_class,
                    stable_identity_requested,
                ) && let Some(acquisition) =
                    build_acquisition_request(&question, &budget, source, *evidence_class)?
                {
                    external_requests.push(GeoPlanExternalRequest::Acquisition {
                        request: acquisition,
                        handoff: GeoPlanAcquisitionHandoff {
                            expected_receipt_contract: CANON_GEO_ACQUISITION_RECEIPT_VERSION
                                .to_string(),
                            required_result_digest_algorithm: GeoDigestAlgorithm::Blake3,
                            continuation_command: PLAN_COMMAND.to_string(),
                        },
                    });
                    actionable_request_count += 1;
                    continue;
                }
                if let Some(source) = unusable_local_source(
                    &inventory,
                    grain.entity_level,
                    *evidence_class,
                    stable_identity_requested,
                ) {
                    diagnostics.push(format!(
                        "source instance {} has local evidence under an unusable contract; no acquisition request was emitted because inventory advancement cannot overwrite an existing artifact reference",
                        source.source_instance_id
                    ));
                    non_overwriting_repair_required = true;
                    continue;
                }
                for gap in support.discovery_gaps.iter().filter(|gap| {
                    gap.requested_entity_level == Some(grain.entity_level)
                        && gap.requested_evidence_class == *evidence_class
                }) {
                    if let Some(discovery) =
                        build_discovery_request(&question, &budget, gap, grain.entity_level)?
                    {
                        external_requests.push(GeoPlanExternalRequest::Discovery {
                            gap_id: gap.gap_id.clone(),
                            request: discovery,
                        });
                        actionable_request_count += 1;
                    } else {
                        external_requests
                            .push(GeoPlanExternalRequest::DiscoveryGap { gap: gap.clone() });
                    }
                }
            }
            outcomes.push(GeoPlanGrainOutcome {
                entity_level: grain.entity_level,
                status: if actionable_request_count > 0 {
                    GeoPlanGrainStatus::WaitingForAcquisition
                } else {
                    GeoPlanGrainStatus::UnsupportedByInventory
                },
                missing_evidence_classes: support_row.missing_evidence_classes.clone(),
                project_node_ids: Vec::new(),
                claim_limitation: "candidate construction cannot begin until every required evidence class is locally available for the bounded geography".to_string(),
                next_action: if actionable_request_count > 0 {
                    "satisfy the emitted typed discovery or acquisition request and add its verified local artifact to the regional inventory".to_string()
                } else if non_overwriting_repair_required {
                    "register the usable local artifact as a distinct versioned source instance or use an explicit artifact-migration workflow; Canon will not overwrite the existing artifact reference".to_string()
                } else {
                    "supply an as-of domain and satisfy the emitted discovery gap without assuming that a parcel or other named source exists".to_string()
                },
            });
            continue;
        }

        validate_supported_grain_budget(&budget)?;

        let required_commands = [
            (HOME_CELLS_COMMAND, CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION),
            (TILE_WORK_COMMAND, CANON_GEO_TILE_WORK_UNIT_VERSION),
            (
                MATERIALIZE_EVIDENCE_COMMAND,
                CANON_GEO_EVIDENCE_REQUEST_VERSION,
            ),
            (
                COMPILE_EVIDENCE_COMMAND,
                CANON_GEO_EVIDENCE_COMPILATION_VERSION,
            ),
            (PROPAGATE_COMMAND, CANON_GEO_PROPAGATION_VERSION),
            (SOLVE_COMMAND, CANON_GEO_COMPOSITION_VERSION),
        ];
        let missing_commands = required_commands
            .iter()
            .filter(|(command, contract)| {
                !implemented_command_matches(&capabilities, command, contract)
            })
            .map(|(command, _)| *command)
            .collect::<Vec<_>>();
        if !missing_commands.is_empty() {
            diagnostics.push(format!(
                "{} planning stopped because required leaf commands are unavailable: {}",
                level_name(grain.entity_level),
                missing_commands.join(", ")
            ));
            outcomes.push(GeoPlanGrainOutcome {
                entity_level: grain.entity_level,
                status: GeoPlanGrainStatus::MissingLeafCapability,
                missing_evidence_classes: Vec::new(),
                project_node_ids: Vec::new(),
                claim_limitation: "no substitute or fabricated command was scheduled".to_string(),
                next_action: "install a Canon build whose capability artifact implements the missing leaf contract".to_string(),
            });
            continue;
        }

        let prefix = format!("geo.{}", level_name(grain.entity_level));
        let stages = grain_project_stages(
            &prefix,
            grain.entity_level,
            grain.required_evidence_classes.clone(),
            question.requested_claim_classes.clone(),
            input_refs.clone(),
            &budget.deterministic_bounds,
            &limits,
        );
        let node_ids = stages
            .iter()
            .map(|(node, _)| node.node_id.clone())
            .collect::<Vec<_>>();
        for (node, overlay) in stages {
            project_nodes.push(node);
            geo_nodes.push(overlay);
        }
        outcomes.push(GeoPlanGrainOutcome {
            entity_level: grain.entity_level,
            status: GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse,
            missing_evidence_classes: Vec::new(),
            project_node_ids: node_ids,
            claim_limitation: "truth reach is unverified; the exact residual is only relative to the declared bounded candidate universe until an independent reference proves reach".to_string(),
            next_action: format!("execute the first runnable project node for {} and validate its typed output before advancing", level_name(grain.entity_level)),
        });
    }

    external_requests.sort_by_key(external_request_sort_key);
    external_requests.dedup();
    outcomes.sort_by_key(|outcome| outcome.entity_level);
    geo_nodes.sort_by(|left, right| left.project_node_id.cmp(&right.project_node_id));
    diagnostics.sort();
    diagnostics.dedup();

    let manifest_digest = digest_json(&(
        &question_hash,
        &capabilities_hash,
        &inventory_planning_hash,
        &profile_hash,
        &budget_planning_hash,
    ))?;
    let mut extension_request = ProjectExtensionDagRequest::offline_read_only(
        format!("geo-plan-{}", &manifest_digest[7..23]),
        manifest_digest,
        inventory_planning_hash.clone(),
        project_nodes,
    );
    extension_request.plan_artifact_path = None;
    let project_plan = compile_extension_project_plan(extension_request).map_err(project_error)?;
    validate_project_plan(&project_plan).map_err(project_error)?;
    validate_overlay_bijection(&project_plan, &geo_nodes)?;

    let planned = outcomes
        .iter()
        .filter(|outcome| outcome.status == GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse)
        .count();
    let waiting = outcomes
        .iter()
        .any(|outcome| outcome.status == GeoPlanGrainStatus::WaitingForAcquisition);
    let status = if planned == outcomes.len() {
        GeoPlanStatus::Planned
    } else if planned > 0 || waiting {
        GeoPlanStatus::Partial
    } else {
        GeoPlanStatus::Unsupported
    };

    let mut plan = GeoPlan {
        version: CANON_GEO_PLAN_VERSION.to_string(),
        plan_id: String::new(),
        semantic_hash: String::new(),
        status,
        question_ref: GeoPlanArtifactRef {
            artifact_id: question.question_id.clone(),
            semantic_hash: question_hash,
        },
        capabilities_ref: GeoPlanArtifactRef {
            artifact_id: capabilities.crate_version.clone(),
            semantic_hash: capabilities_hash,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: inventory.inventory_id.clone(),
            semantic_hash: inventory_hash,
            planning_hash: inventory_planning_hash,
        },
        profile_ref: GeoPlanProfileRef {
            version: profile.version,
            selection_level: profile.selection_level,
            semantic_hash: profile_hash,
        },
        budget_ref: GeoPlanBudgetRef {
            budget_id: budget.budget_id,
            semantic_hash: budget_hash,
            planning_hash: budget_planning_hash,
        },
        project_plan,
        geo_nodes,
        grain_outcomes: outcomes,
        external_requests,
        diagnostics,
    };
    plan.semantic_hash = geo_plan_semantic_hash(&plan)?;
    plan.plan_id = format!(
        "{CANON_GEO_PLAN_VERSION}:{}",
        plan.semantic_hash.trim_start_matches("blake3:")
    );
    validate_geo_plan(&plan)?;
    Ok(plan)
}

pub fn replan_geo_plan_from_inventory_advancement(
    request: GeoPlanReplanRequest,
) -> Result<GeoPlan, GeoPlanError> {
    validate_geo_plan(&request.base_plan)?;
    let base_inventory =
        validate_replan_base_inventory(&request.base_plan, &request.base_inventory)?;
    validate_replan_artifact_inputs(
        &request.base_plan,
        &request.question,
        &request.capabilities,
        &request.profile,
        &request.budget,
    )?;
    let advanced_inventory = validate_inventory_advancement_for_replan(
        &request.base_plan,
        &base_inventory,
        &request.question,
        &request.inventory_advancement,
    )?;
    let advanced_planning_hash =
        regional_inventory_planning_hash(&advanced_inventory).map_err(control_error)?;
    if advanced_planning_hash == request.base_plan.inventory_ref.planning_hash {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::InvalidInput,
            "Geo replan requires an advanced regional inventory that changes planning identity",
            [("inventory_planning_hash", advanced_planning_hash.as_str())],
        ));
    }

    let replanned = compile_geo_plan(GeoPlanRequest {
        question: request.question,
        capabilities: request.capabilities,
        inventory: advanced_inventory,
        profile: request.profile,
        budget: request.budget,
    })?;
    if replanned.plan_id == request.base_plan.plan_id {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan must publish a new immutable plan identity",
            [("plan_id", replanned.plan_id.as_str())],
        ));
    }
    Ok(replanned)
}

pub fn canonical_geo_plan_bytes(plan: &GeoPlan) -> Result<Vec<u8>, GeoPlanError> {
    validate_geo_plan(plan)?;
    let mut canonical = plan.clone();
    for external_request in &mut canonical.external_requests {
        canonicalize_plan_external_request(external_request);
    }
    canonical
        .external_requests
        .sort_by_key(external_request_sort_key);
    canonical.diagnostics.sort();
    canonical.diagnostics.dedup();
    serde_json::to_vec(&canonical).map_err(serialization_error)
}

pub fn validate_geo_plan(plan: &GeoPlan) -> Result<(), GeoPlanError> {
    if plan.version != CANON_GEO_PLAN_VERSION {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::UnsupportedVersion,
            "unsupported Geo plan version",
            [("version", plan.version.as_str())],
        ));
    }
    validate_project_plan(&plan.project_plan).map_err(project_error)?;
    validate_overlay_bijection(&plan.project_plan, &plan.geo_nodes)?;
    for node in &plan.geo_nodes {
        validate_node_cost_contract(plan, node)?;
        if node.stage == GeoPlanStage::FactorAndSolveExactResidual {
            if !node.bounded_section_required || !node.incidence_factorization_required {
                return Err(GeoPlanError::new(
                    GeoPlanErrorCode::ContractViolation,
                    "exact solve nodes require a bounded section and incidence factorization",
                    [("project_node_id", node.project_node_id.as_str())],
                ));
            }
            if node.preconditions.iter().any(|precondition| {
                matches!(
                    precondition.plane,
                    GeoPlanGatePlane::Coverage | GeoPlanGatePlane::CandidateReach
                ) && precondition.status == GeoPlanGateStatus::FailedAgainstReference
            }) {
                return Err(GeoPlanError::new(
                    GeoPlanErrorCode::ContractViolation,
                    "failed coverage or candidate reach must stop the grain before exact solving",
                    [("project_node_id", node.project_node_id.as_str())],
                ));
            }
            let Some(scope) = &node.exact_solve_scope else {
                return Err(GeoPlanError::new(
                    GeoPlanErrorCode::ContractViolation,
                    "exact solve nodes require explicit bounded-section and component scope",
                    [("project_node_id", node.project_node_id.as_str())],
                ));
            };
            validate_solve_scope(plan, node, scope)?;
        } else if node.bounded_section_required
            || node.incidence_factorization_required
            || node.exact_solve_scope.is_some()
        {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "only exact solve nodes may declare exact-solve scope",
                [("project_node_id", node.project_node_id.as_str())],
            ));
        }
    }
    for request in &plan.external_requests {
        match request {
            GeoPlanExternalRequest::Acquisition { request, handoff } => {
                validate_geo_acquisition_request(request).map_err(discovery_error)?;
                if handoff.expected_receipt_contract != CANON_GEO_ACQUISITION_RECEIPT_VERSION
                    || handoff.required_result_digest_algorithm != GeoDigestAlgorithm::Blake3
                    || handoff.continuation_command != PLAN_COMMAND
                {
                    return Err(GeoPlanError::new(
                        GeoPlanErrorCode::ContractViolation,
                        "Geo acquisition handoff must require the versioned receipt, BLAKE3 result digests, and exact planner continuation",
                        [("request_id", request.request_id.as_str())],
                    ));
                }
            }
            GeoPlanExternalRequest::Discovery { gap_id, request } => {
                if gap_id.trim().is_empty() || gap_id.trim() != gap_id {
                    return Err(GeoPlanError::invalid(
                        "Geo discovery request gap ids must be non-empty and trimmed",
                    ));
                }
                validate_geo_discovery_request(request).map_err(discovery_error)?;
            }
            GeoPlanExternalRequest::DiscoveryGap { gap } => {
                if gap.gap_id.trim().is_empty()
                    || gap.reason.trim().is_empty()
                    || gap.next_command.trim().is_empty()
                {
                    return Err(GeoPlanError::invalid(
                        "Geo discovery gaps require an id, reason, and next command",
                    ));
                }
            }
        }
    }
    let expected_hash = geo_plan_semantic_hash(plan)?;
    if plan.semantic_hash != expected_hash {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo plan semantic_hash does not match its semantic projection",
            [
                ("expected", expected_hash.as_str()),
                ("actual", plan.semantic_hash.as_str()),
            ],
        ));
    }
    let expected_id = format!(
        "{CANON_GEO_PLAN_VERSION}:{}",
        expected_hash.trim_start_matches("blake3:")
    );
    if plan.plan_id != expected_id {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo plan plan_id does not match semantic_hash",
            [("expected", expected_id), ("actual", plan.plan_id.clone())],
        ));
    }
    Ok(())
}

fn validate_replan_artifact_inputs(
    base_plan: &GeoPlan,
    question: &GeoQuestion,
    capabilities: &GeoCapabilities,
    profile: &GeoCompositionProfile,
    budget: &GeoResourceBudget,
) -> Result<(), GeoPlanError> {
    let question = canonicalize_question(question).map_err(control_error)?;
    let question_hash = question_semantic_hash(&question).map_err(control_error)?;
    validate_replan_ref(
        "question",
        &base_plan.question_ref.artifact_id,
        &question.question_id,
    )?;
    validate_replan_ref(
        "question_semantic_hash",
        &base_plan.question_ref.semantic_hash,
        &question_hash,
    )?;

    let capabilities = canonicalize_capabilities(capabilities).map_err(control_error)?;
    let capabilities_hash = capabilities_semantic_hash(&capabilities).map_err(control_error)?;
    validate_replan_ref(
        "capabilities",
        &base_plan.capabilities_ref.artifact_id,
        &capabilities.crate_version,
    )?;
    validate_replan_ref(
        "capabilities_semantic_hash",
        &base_plan.capabilities_ref.semantic_hash,
        &capabilities_hash,
    )?;

    let profile = validate_profile(profile)?;
    let profile_hash = digest_json(&profile)?;
    validate_replan_ref(
        "profile_version",
        &base_plan.profile_ref.version,
        &profile.version,
    )?;
    if base_plan.profile_ref.selection_level != profile.selection_level {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan typed inputs must match the base plan profile",
            [
                ("field".to_string(), "profile_selection_level".to_string()),
                (
                    "expected".to_string(),
                    format!("{:?}", base_plan.profile_ref.selection_level),
                ),
                (
                    "actual".to_string(),
                    format!("{:?}", profile.selection_level),
                ),
            ],
        ));
    }
    validate_replan_ref(
        "profile_semantic_hash",
        &base_plan.profile_ref.semantic_hash,
        &profile_hash,
    )?;

    let budget = canonicalize_resource_budget(budget).map_err(control_error)?;
    let budget_hash = resource_budget_semantic_hash(&budget).map_err(control_error)?;
    let budget_planning_hash = deterministic_budget_planning_hash(&budget)?;
    validate_replan_ref("budget", &base_plan.budget_ref.budget_id, &budget.budget_id)?;
    validate_replan_ref(
        "budget_semantic_hash",
        &base_plan.budget_ref.semantic_hash,
        &budget_hash,
    )?;
    validate_replan_ref(
        "budget_planning_hash",
        &base_plan.budget_ref.planning_hash,
        &budget_planning_hash,
    )?;

    Ok(())
}

fn validate_inventory_advancement_for_replan(
    base_plan: &GeoPlan,
    base_inventory: &GeoRegionalInventory,
    question: &GeoQuestion,
    advancement: &GeoRegionalInventoryAdvancement,
) -> Result<GeoRegionalInventory, GeoPlanError> {
    validate_geo_regional_inventory_advancement(advancement).map_err(satisfy_error)?;
    if advancement.version != CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::UnsupportedVersion,
            "unsupported Geo regional inventory advancement version",
            [
                ("actual", advancement.version.as_str()),
                ("expected", CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION),
            ],
        ));
    }
    if advancement.effect != GeoInventoryAdvancementEffect::LocalAvailabilityOnly {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan accepts only local-availability inventory advancements",
            [("effect", format!("{:?}", advancement.effect))],
        ));
    }
    if advancement.proof_class != super::GeoAcquisitionProofClass::Live
        || advancement.receipt_terminal_state != super::GeoAcquisitionTerminalState::Complete
    {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan requires a live complete acquisition advancement",
            [
                ("proof_class", format!("{:?}", advancement.proof_class)),
                (
                    "terminal_state",
                    format!("{:?}", advancement.receipt_terminal_state),
                ),
            ],
        ));
    }
    if advancement.source_advancements.is_empty() {
        return Err(GeoPlanError::invalid(
            "Geo replan requires at least one source advancement",
        ));
    }
    let mut sorted_source_advancements = advancement.source_advancements.clone();
    sorted_source_advancements.sort();
    sorted_source_advancements.dedup();
    if sorted_source_advancements != advancement.source_advancements {
        return Err(GeoPlanError::invalid(
            "Geo replan source advancements must be sorted and distinct",
        ));
    }

    validate_replan_ref("plan_id", &base_plan.plan_id, &advancement.plan_id)?;
    validate_replan_ref(
        "plan_semantic_hash",
        &base_plan.semantic_hash,
        &advancement.plan_semantic_hash,
    )?;
    validate_replan_ref(
        "base_inventory_id",
        &base_plan.inventory_ref.inventory_id,
        &advancement.base_inventory_id,
    )?;
    validate_replan_ref(
        "base_inventory_semantic_hash",
        &base_plan.inventory_ref.semantic_hash,
        &advancement.base_inventory_semantic_hash,
    )?;

    let question = canonicalize_question(question).map_err(control_error)?;
    if advancement.bounded_geography != question.bounded_geography {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan advancement geography must match the base question",
            [
                (
                    "question_geography_id",
                    question.bounded_geography.geography_id.as_str(),
                ),
                (
                    "advancement_geography_id",
                    advancement.bounded_geography.geography_id.as_str(),
                ),
            ],
        ));
    }

    let advanced_inventory =
        canonicalize_regional_inventory(&advancement.advanced_inventory).map_err(control_error)?;
    if advanced_inventory.region != advancement.bounded_geography {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan advanced inventory region must match the advancement geography",
            [
                (
                    "advanced_inventory_region",
                    advanced_inventory.region.geography_id.as_str(),
                ),
                (
                    "advancement_geography_id",
                    advancement.bounded_geography.geography_id.as_str(),
                ),
            ],
        ));
    }
    let advanced_inventory_semantic_hash =
        regional_inventory_semantic_hash(&advanced_inventory).map_err(control_error)?;
    validate_replan_ref(
        "advanced_inventory_id",
        &advanced_inventory.inventory_id,
        &advancement.advanced_inventory_id,
    )?;
    validate_replan_ref(
        "advanced_inventory_semantic_hash",
        &advanced_inventory_semantic_hash,
        &advancement.advanced_inventory_semantic_hash,
    )?;
    if advancement.advanced_inventory_semantic_hash == base_plan.inventory_ref.semantic_hash {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::InvalidInput,
            "Geo replan requires an advanced inventory distinct from the base inventory",
            [(
                "inventory_semantic_hash",
                advancement.advanced_inventory_semantic_hash.as_str(),
            )],
        ));
    }

    let bounded_subset_hash = digest_json(&advancement.bounded_subset)?;
    validate_replan_ref(
        "bounded_subset_hash",
        &bounded_subset_hash,
        &advancement.bounded_subset_hash,
    )?;

    let expected_advancement_hash =
        geo_regional_inventory_advancement_semantic_hash(advancement).map_err(satisfy_error)?;
    validate_replan_ref(
        "inventory_advancement_semantic_hash",
        &expected_advancement_hash,
        &advancement.semantic_hash,
    )?;
    let expected_advancement_id = format!(
        "{CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION}:{}",
        expected_advancement_hash.trim_start_matches("blake3:")
    );
    validate_replan_ref(
        "inventory_advancement_id",
        &expected_advancement_id,
        &advancement.advancement_id,
    )?;

    let acquisition_request =
        validate_advancement_matches_base_acquisition(base_plan, advancement)?;
    if canonicalize_replan_subset(&advancement.bounded_subset)
        != canonicalize_replan_subset(&acquisition_request.subset)
    {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan bounded subset must match the base acquisition request",
            [("field", "bounded_subset")],
        ));
    }
    validate_source_advancements(acquisition_request, &advanced_inventory, advancement)?;
    validate_advanced_inventory_transition(base_inventory, &advanced_inventory, advancement)?;
    Ok(advanced_inventory)
}

fn validate_replan_base_inventory(
    base_plan: &GeoPlan,
    inventory: &GeoRegionalInventory,
) -> Result<GeoRegionalInventory, GeoPlanError> {
    let inventory = canonicalize_regional_inventory(inventory).map_err(control_error)?;
    let semantic_hash = regional_inventory_semantic_hash(&inventory).map_err(control_error)?;
    let planning_hash = regional_inventory_planning_hash(&inventory).map_err(control_error)?;
    validate_replan_ref(
        "base_inventory_id",
        &base_plan.inventory_ref.inventory_id,
        &inventory.inventory_id,
    )?;
    validate_replan_ref(
        "base_inventory_semantic_hash",
        &base_plan.inventory_ref.semantic_hash,
        &semantic_hash,
    )?;
    validate_replan_ref(
        "base_inventory_planning_hash",
        &base_plan.inventory_ref.planning_hash,
        &planning_hash,
    )?;
    Ok(inventory)
}

fn validate_advancement_matches_base_acquisition<'a>(
    base_plan: &'a GeoPlan,
    advancement: &GeoRegionalInventoryAdvancement,
) -> Result<&'a GeoAcquisitionRequest, GeoPlanError> {
    let mut matches = 0_usize;
    let mut matched_request = None;
    for request in &base_plan.external_requests {
        let GeoPlanExternalRequest::Acquisition { request, .. } = request else {
            continue;
        };
        if request.request_id != advancement.request_id {
            continue;
        }
        let request_hash =
            geo_acquisition_request_semantic_hash(request).map_err(discovery_error)?;
        validate_replan_ref(
            "request_semantic_hash",
            &request_hash,
            &advancement.request_semantic_hash,
        )?;
        matches += 1;
        matched_request = Some(request);
    }
    if matches != 1 {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan advancement must match exactly one acquisition request in the base plan",
            [
                ("request_id".to_string(), advancement.request_id.clone()),
                ("matches".to_string(), matches.to_string()),
            ],
        ));
    }
    Ok(matched_request.expect("matches == 1"))
}

fn validate_source_advancements(
    request: &GeoAcquisitionRequest,
    advanced_inventory: &GeoRegionalInventory,
    advancement: &GeoRegionalInventoryAdvancement,
) -> Result<(), GeoPlanError> {
    let release_keys = request
        .releases
        .iter()
        .map(release_pin_key)
        .collect::<Result<BTreeSet<_>, _>>()?;
    let advanced_release_keys = advancement
        .source_advancements
        .iter()
        .map(source_advancement_key)
        .collect::<BTreeSet<_>>();
    if advanced_release_keys != release_keys {
        let missing_release_keys = release_keys
            .difference(&advanced_release_keys)
            .map(|key| format!("{}:{}:{}", key.0, key.1, key.2))
            .collect::<Vec<_>>();
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo replan source advancements must cover every pinned acquisition release",
            [
                (
                    "expected_release_count".to_string(),
                    release_keys.len().to_string(),
                ),
                (
                    "advanced_release_count".to_string(),
                    advanced_release_keys.len().to_string(),
                ),
                (
                    "missing_release_keys".to_string(),
                    missing_release_keys.join(","),
                ),
            ],
        ));
    }
    for digest in &advancement.result_digests {
        if digest.algorithm != GeoDigestAlgorithm::Blake3 {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan result digests must use the plan handoff's BLAKE3 algorithm",
                [
                    ("digest_id".to_string(), digest.digest_id.clone()),
                    ("algorithm".to_string(), format!("{:?}", digest.algorithm)),
                ],
            ));
        }
    }
    for source_advancement in &advancement.source_advancements {
        let source_key = source_advancement_key(source_advancement);
        if !release_keys.contains(&source_key) {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancement does not match the base acquisition release pin",
                [
                    (
                        "source_instance_id".to_string(),
                        source_advancement.source_instance_id.clone(),
                    ),
                    (
                        "release_id".to_string(),
                        source_advancement.release.release_id.clone(),
                    ),
                    (
                        "release_digest".to_string(),
                        source_advancement.release.release_digest.clone(),
                    ),
                ],
            ));
        }
        if source_advancement.advanced_state != super::GeoSourceAvailability::Available {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancements must end in local availability",
                [
                    (
                        "source_instance_id".to_string(),
                        source_advancement.source_instance_id.clone(),
                    ),
                    (
                        "advanced_state".to_string(),
                        format!("{:?}", source_advancement.advanced_state),
                    ),
                ],
            ));
        }
        if source_advancement.local_artifact_byte_count == 0 {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancements must bind non-empty local artifacts",
                [(
                    "source_instance_id",
                    source_advancement.source_instance_id.as_str(),
                )],
            ));
        }
        if source_advancement
            .local_artifact_contract_version
            .as_deref()
            != Some(source_advancement.local_ref.contract_version.as_str())
        {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancement contract must match its local artifact ref",
                [(
                    "source_instance_id",
                    source_advancement.source_instance_id.as_str(),
                )],
            ));
        }
        let mut sorted_result_digest_ids = source_advancement.result_digest_ids.clone();
        sorted_result_digest_ids.sort();
        sorted_result_digest_ids.dedup();
        if sorted_result_digest_ids.is_empty()
            || sorted_result_digest_ids != source_advancement.result_digest_ids
        {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancements require sorted distinct result digest ids",
                [(
                    "source_instance_id",
                    source_advancement.source_instance_id.as_str(),
                )],
            ));
        }
        for digest_id in &source_advancement.result_digest_ids {
            let matches = advancement
                .result_digests
                .iter()
                .filter(|digest| digest.digest_id == *digest_id)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(GeoPlanError::new(
                    GeoPlanErrorCode::ContractViolation,
                    "Geo replan source advancement result digest id must resolve exactly once",
                    [
                        (
                            "source_instance_id".to_string(),
                            source_advancement.source_instance_id.clone(),
                        ),
                        ("result_digest_id".to_string(), digest_id.clone()),
                        ("matches".to_string(), matches.len().to_string()),
                    ],
                ));
            }
            let result_digest = geo_digest_string(matches[0])?;
            if result_digest != source_advancement.local_ref.content_hash {
                return Err(GeoPlanError::new(
                    GeoPlanErrorCode::ContractViolation,
                    "Geo replan source advancement local artifact must match its receipt result digest",
                    [
                        (
                            "source_instance_id".to_string(),
                            source_advancement.source_instance_id.clone(),
                        ),
                        ("result_digest_id".to_string(), digest_id.clone()),
                        ("expected".to_string(), result_digest),
                        (
                            "actual".to_string(),
                            source_advancement.local_ref.content_hash.clone(),
                        ),
                    ],
                ));
            }
        }
        let Some(source) = advanced_inventory.sources.iter().find(|source| {
            source.source_instance_id == source_advancement.source_instance_id
                && source.release == source_advancement.release
                && source.coverage.region == advancement.bounded_geography
        }) else {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancement is missing from the advanced inventory",
                [(
                    "source_instance_id",
                    source_advancement.source_instance_id.as_str(),
                )],
            ));
        };
        if source.local_state.state != super::GeoSourceAvailability::Available
            || source.local_state.local_ref.as_ref() != Some(&source_advancement.local_ref)
        {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan advanced inventory must carry each validated local artifact ref",
                [(
                    "source_instance_id",
                    source_advancement.source_instance_id.as_str(),
                )],
            ));
        }
    }
    Ok(())
}

fn validate_advanced_inventory_transition(
    base_inventory: &GeoRegionalInventory,
    advanced_inventory: &GeoRegionalInventory,
    advancement: &GeoRegionalInventoryAdvancement,
) -> Result<(), GeoPlanError> {
    let mut expected = base_inventory.clone();
    let mut seen = BTreeSet::new();
    for source_advancement in &advancement.source_advancements {
        let key = source_advancement_key(source_advancement);
        if !seen.insert(key.clone()) {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancements must name each source release at most once",
                [
                    ("source_instance_id".to_string(), key.0),
                    ("release_id".to_string(), key.1),
                    ("release_digest".to_string(), key.2),
                ],
            ));
        }
        let Some(source) = expected.sources.iter_mut().find(|source| {
            source.source_instance_id == source_advancement.source_instance_id
                && source.release == source_advancement.release
                && source.coverage.region == advancement.bounded_geography
        }) else {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancement is missing from the base inventory",
                [
                    (
                        "source_instance_id".to_string(),
                        source_advancement.source_instance_id.clone(),
                    ),
                    (
                        "release_id".to_string(),
                        source_advancement.release.release_id.clone(),
                    ),
                ],
            ));
        };
        if source.local_state.state != source_advancement.previous_state {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancement previous state does not match the base inventory",
                [
                    (
                        "source_instance_id".to_string(),
                        source_advancement.source_instance_id.clone(),
                    ),
                    (
                        "expected".to_string(),
                        format!("{:?}", source.local_state.state),
                    ),
                    (
                        "actual".to_string(),
                        format!("{:?}", source_advancement.previous_state),
                    ),
                ],
            ));
        }
        if let Some(existing_ref) = &source.local_state.local_ref
            && existing_ref != &source_advancement.local_ref
        {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "Geo replan source advancement would overwrite an existing local artifact ref",
                [
                    (
                        "source_instance_id".to_string(),
                        source_advancement.source_instance_id.clone(),
                    ),
                    (
                        "existing_artifact_id".to_string(),
                        existing_ref.artifact_id.clone(),
                    ),
                    (
                        "advanced_artifact_id".to_string(),
                        source_advancement.local_ref.artifact_id.clone(),
                    ),
                ],
            ));
        }
        source.local_state.state = super::GeoSourceAvailability::Available;
        source.local_state.local_ref = Some(source_advancement.local_ref.clone());
    }

    let expected = canonicalize_regional_inventory(&expected).map_err(control_error)?;
    if &expected == advanced_inventory {
        return Ok(());
    }

    let expected_semantic_hash =
        regional_inventory_semantic_hash(&expected).map_err(control_error)?;
    let actual_semantic_hash =
        regional_inventory_semantic_hash(advanced_inventory).map_err(control_error)?;
    let expected_planning_hash =
        regional_inventory_planning_hash(&expected).map_err(control_error)?;
    let actual_planning_hash =
        regional_inventory_planning_hash(advanced_inventory).map_err(control_error)?;
    Err(GeoPlanError::new(
        GeoPlanErrorCode::ContractViolation,
        "Geo replan advanced inventory must equal the base inventory plus declared source advancements",
        [
            (
                "field".to_string(),
                "advanced_inventory_transition".to_string(),
            ),
            ("expected_semantic_hash".to_string(), expected_semantic_hash),
            ("actual_semantic_hash".to_string(), actual_semantic_hash),
            ("expected_planning_hash".to_string(), expected_planning_hash),
            ("actual_planning_hash".to_string(), actual_planning_hash),
        ],
    ))
}

fn release_pin_key(
    release: &super::GeoReleasePin,
) -> Result<(String, String, String), GeoPlanError> {
    Ok((
        release.source_instance_id.clone(),
        release.release_id.clone(),
        geo_digest_string(&release.release_digest)?,
    ))
}

fn source_advancement_key(
    source_advancement: &GeoRegionalInventorySourceAdvancement,
) -> (String, String, String) {
    (
        source_advancement.source_instance_id.clone(),
        source_advancement.release.release_id.clone(),
        source_advancement.release.release_digest.clone(),
    )
}

fn geo_digest_string(digest: &GeoDigest) -> Result<String, GeoPlanError> {
    if digest.algorithm != GeoDigestAlgorithm::Blake3 {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo planning expected a BLAKE3 digest",
            [("digest_id", digest.digest_id.as_str())],
        ));
    }
    Ok(format!("blake3:{}", digest.hex_digest))
}

fn validate_replan_ref(field: &str, expected: &str, actual: &str) -> Result<(), GeoPlanError> {
    if expected == actual {
        return Ok(());
    }
    Err(GeoPlanError::new(
        GeoPlanErrorCode::ContractViolation,
        "Geo replan input does not match the base plan or advancement",
        [
            ("field", field.to_string()),
            ("expected", expected.to_string()),
            ("actual", actual.to_string()),
        ],
    ))
}

pub fn geo_plan_semantic_hash(plan: &GeoPlan) -> Result<String, GeoPlanError> {
    let mut external_request_planning_hashes = plan
        .external_requests
        .iter()
        .map(external_request_planning_hash)
        .collect::<Result<Vec<_>, _>>()?;
    external_request_planning_hashes.sort();
    let geo_nodes = plan
        .geo_nodes
        .iter()
        .map(|node| GeoPlanNodeSemanticProjection {
            project_node_id: &node.project_node_id,
            stage: node.stage,
            entity_level: node.entity_level,
            evidence_classes: &node.evidence_classes,
            claim_classes: &node.claim_classes,
            expected_output_contract: &node.expected_output_contract,
            preconditions: node
                .preconditions
                .iter()
                .map(|precondition| GeoPlanPreconditionSemanticProjection {
                    plane: precondition.plane,
                    status: precondition.status,
                })
                .collect(),
            claim_effect: node.claim_effect,
            bounded_section_required: node.bounded_section_required,
            incidence_factorization_required: node.incidence_factorization_required,
            exact_solve_scope: &node.exact_solve_scope,
            deterministic_bounds: &node.deterministic_bounds,
        })
        .collect();
    let grain_outcomes = plan
        .grain_outcomes
        .iter()
        .map(|outcome| GeoPlanGrainOutcomeSemanticProjection {
            entity_level: outcome.entity_level,
            status: outcome.status,
            missing_evidence_classes: &outcome.missing_evidence_classes,
            project_node_ids: &outcome.project_node_ids,
        })
        .collect();
    let projection = GeoPlanSemanticProjection {
        version: &plan.version,
        question_hash: &plan.question_ref.semantic_hash,
        capabilities_hash: &plan.capabilities_ref.semantic_hash,
        inventory_planning_hash: &plan.inventory_ref.planning_hash,
        profile_hash: &plan.profile_ref.semantic_hash,
        budget_planning_hash: &plan.budget_ref.planning_hash,
        project_graph_hash: &plan.project_plan.graph_hash,
        geo_nodes,
        grain_outcomes,
        external_request_planning_hashes,
    };
    digest_json(&projection)
}

fn external_request_planning_hash(
    request: &GeoPlanExternalRequest,
) -> Result<String, GeoPlanError> {
    match request {
        GeoPlanExternalRequest::Acquisition { request, handoff } => {
            let canonical = canonicalize_geo_acquisition_request(request);
            let releases = canonical
                .releases
                .iter()
                .map(|release| GeoAcquisitionReleasePlanningRef {
                    release_id: &release.release_id,
                    release_digest: &release.release_digest,
                })
                .collect();
            digest_json(&GeoAcquisitionPlanningProjection {
                version: &canonical.version,
                bounded_geography: &canonical.bounded_geography,
                subset: &canonical.subset,
                releases,
                fields: &canonical.fields,
                projection: &canonical.projection,
                ordering: &canonical.ordering,
                pagination: &canonical.pagination,
                ceilings: &canonical.ceilings,
                positive_path_min_rows: canonical.positive_path_min_rows,
                expected_receipt_contract: &handoff.expected_receipt_contract,
                required_result_digest_algorithm: handoff.required_result_digest_algorithm,
            })
        }
        GeoPlanExternalRequest::Discovery { request, .. } => {
            let canonical = canonicalize_geo_discovery_request(request);
            let releases = canonical
                .releases
                .iter()
                .map(|release| GeoAcquisitionReleasePlanningRef {
                    release_id: &release.release_id,
                    release_digest: &release.release_digest,
                })
                .collect();
            digest_json(&GeoDiscoveryPlanningProjection {
                version: &canonical.version,
                bounded_geography: &canonical.bounded_geography,
                subset: &canonical.subset,
                requested_entity_levels: &canonical.requested_entity_levels,
                requested_evidence_classes: &canonical.requested_evidence_classes,
                release_selection: &canonical.release_selection,
                releases,
                fields: &canonical.fields,
                required_steps: &canonical.required_steps,
                readability_fields: &canonical.column_readability_probe.fields,
                readability_subset: &canonical.column_readability_probe.subset,
                readability_ceilings: &canonical.column_readability_probe.ceilings,
                ceilings: &canonical.ceilings,
            })
        }
        GeoPlanExternalRequest::DiscoveryGap { gap } => digest_json(&(
            "discovery_gap",
            gap.requested_entity_level,
            gap.requested_evidence_class,
        )),
    }
}

fn implemented_command_matches(
    capabilities: &GeoCapabilities,
    command: &str,
    output_contract: &str,
) -> bool {
    capabilities.commands.implemented.iter().any(|candidate| {
        candidate.command == command
            && candidate.output_contract == output_contract
            && candidate.read_only
            && !candidate.uses_network
    })
}

fn require_implemented_command(
    capabilities: &GeoCapabilities,
    command: &str,
    output_contract: &str,
) -> Result<(), GeoPlanError> {
    if implemented_command_matches(capabilities, command, output_contract) {
        return Ok(());
    }
    Err(GeoPlanError::new(
        GeoPlanErrorCode::MissingCapability,
        "Geo planning requires an implemented offline read-only command with the exact output contract",
        [("command", command), ("output_contract", output_contract)],
    ))
}

fn validate_profile(
    profile: &GeoCompositionProfile,
) -> Result<GeoCompositionProfile, GeoPlanError> {
    validate_composition_profile(profile).map_err(composition_profile_error)
}

fn deterministic_budget_planning_hash(budget: &GeoResourceBudget) -> Result<String, GeoPlanError> {
    #[derive(Serialize)]
    struct Projection<'a> {
        version: &'a str,
        budget_id: &'a str,
        deterministic_bounds: &'a [super::GeoNumericBound],
    }
    digest_json(&Projection {
        version: &budget.version,
        budget_id: &budget.budget_id,
        deterministic_bounds: &budget.deterministic_bounds,
    })
}

fn profile_control_level(profile: &GeoCompositionProfile) -> Option<GeoControlEntityLevel> {
    match profile.selection_level {
        GeoEntityLevel::Parcel => Some(GeoControlEntityLevel::Parcel),
        GeoEntityLevel::Building => Some(GeoControlEntityLevel::Building),
        GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => None,
    }
}

fn acquisition_source(
    inventory: &GeoRegionalInventory,
    level: GeoControlEntityLevel,
    evidence_class: GeoEvidenceClass,
    stable_identity_requested: bool,
) -> Option<&GeoRegionalSourceInstance> {
    inventory.sources.iter().find(|source| {
        matches!(
            source.native_scope,
            GeoNativeEntityScope::NativeEntity { entity_level, .. } if entity_level == level
        ) && source
            .evidence_classes
            .binary_search(&evidence_class)
            .is_ok()
            && (!stable_identity_requested || source.native_scope.may_contribute_stable_alias())
            && !super::regional_source_has_usable_local_evidence(source)
            && source.local_state.local_ref.is_none()
            && source.coverage.region == inventory.region
    })
}

fn unusable_local_source(
    inventory: &GeoRegionalInventory,
    level: GeoControlEntityLevel,
    evidence_class: GeoEvidenceClass,
    stable_identity_requested: bool,
) -> Option<&GeoRegionalSourceInstance> {
    inventory.sources.iter().find(|source| {
        matches!(
            source.native_scope,
            GeoNativeEntityScope::NativeEntity { entity_level, .. } if entity_level == level
        ) && source
            .evidence_classes
            .binary_search(&evidence_class)
            .is_ok()
            && (!stable_identity_requested || source.native_scope.may_contribute_stable_alias())
            && !super::regional_source_has_usable_local_evidence(source)
            && source.local_state.local_ref.is_some()
            && source.coverage.region == inventory.region
    })
}

fn build_acquisition_request(
    question: &GeoQuestion,
    budget: &GeoResourceBudget,
    source: &GeoRegionalSourceInstance,
    evidence_class: GeoEvidenceClass,
) -> Result<Option<GeoAcquisitionRequest>, GeoPlanError> {
    let Some(max_rows) = budget_limit(budget, GeoResourceCounter::Rows) else {
        return Ok(None);
    };
    let Some(max_bytes) = budget_limit(budget, GeoResourceCounter::Bytes) else {
        return Ok(None);
    };
    if max_rows == 0 || max_bytes == 0 {
        return Ok(None);
    }
    let geometry_required = matches!(
        evidence_class,
        GeoEvidenceClass::GeocodePoint
            | GeoEvidenceClass::ParcelGeometry
            | GeoEvidenceClass::BuildingFootprint
    );
    let mut fields = vec![
        GeoRequestedField {
            field_id: "native_id".to_string(),
            role: GeoFieldRole::Identifier,
            required: true,
        },
        GeoRequestedField {
            field_id: "source_record_digest".to_string(),
            role: GeoFieldRole::Digest,
            required: true,
        },
    ];
    fields.push(GeoRequestedField {
        field_id: evidence_class_field(evidence_class).to_string(),
        role: if geometry_required {
            GeoFieldRole::Geometry
        } else {
            GeoFieldRole::Attribute
        },
        required: true,
    });
    fields.sort();
    fields.dedup();
    let subset = GeoBoundedSubset {
        subset_id: format!("subset:{}", question.bounded_geography.geography_id),
        geography: question.bounded_geography.clone(),
        h3_cells: Vec::new(),
        predicates: vec![GeoSubsetPredicate {
            predicate_id: "question_bounded_geography".to_string(),
            kind: GeoSubsetPredicateKind::AdministrativeBoundary,
            expression: question.bounded_geography.geography_id.clone(),
        }],
    };
    let projection = if geometry_required {
        let Some(geometry) = &source.geometry else {
            return Ok(None);
        };
        Some(GeoProjectionOperation {
            coordinate_reference_system: geometry.coordinate_reference_system.clone(),
            operation_id: geometry.transform_id.clone(),
            operation_version: geometry.geometry_contract_version.clone(),
            operation_digest: digest_contract("transform", &geometry.transform_digest)?,
        })
    } else {
        None
    };
    let mut request = GeoAcquisitionRequest {
        version: CANON_GEO_ACQUISITION_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        discovery_request_id: None,
        bounded_geography: question.bounded_geography.clone(),
        subset,
        releases: vec![super::GeoReleasePin {
            source_instance_id: source.source_instance_id.clone(),
            release_id: source.release.release_id.clone(),
            release_digest: digest_contract("release", &source.release.release_digest)?,
        }],
        fields,
        projection,
        ordering: vec![GeoOrderingTerm {
            position: 0,
            field_id: "native_id".to_string(),
            direction: GeoOrderDirection::Asc,
            nulls: super::GeoNullOrdering::Last,
        }],
        pagination: GeoPaginationRequest {
            page_size_rows: max_rows.min(10_000),
            page_token: None,
        },
        ceilings: GeoRowByteCeilings {
            max_rows,
            max_bytes,
        },
        positive_path_min_rows: 1,
    };
    request.request_id = geo_acquisition_request_id(&request).map_err(discovery_error)?;
    validate_geo_acquisition_request(&request).map_err(discovery_error)?;
    Ok(Some(request))
}

fn build_discovery_request(
    question: &GeoQuestion,
    budget: &GeoResourceBudget,
    gap: &GeoDiscoveryGap,
    entity_level: GeoControlEntityLevel,
) -> Result<Option<GeoDiscoveryRequest>, GeoPlanError> {
    let Some(as_of) = &question.query_as_of else {
        return Ok(None);
    };
    let Some(max_rows) = budget_limit(budget, GeoResourceCounter::Rows) else {
        return Ok(None);
    };
    let Some(max_bytes) = budget_limit(budget, GeoResourceCounter::Bytes) else {
        return Ok(None);
    };
    if max_rows == 0 || max_bytes == 0 {
        return Ok(None);
    }
    let subset = GeoBoundedSubset {
        subset_id: format!("subset:{}", question.bounded_geography.geography_id),
        geography: question.bounded_geography.clone(),
        h3_cells: Vec::new(),
        predicates: vec![GeoSubsetPredicate {
            predicate_id: "question_bounded_geography".to_string(),
            kind: GeoSubsetPredicateKind::AdministrativeBoundary,
            expression: question.bounded_geography.geography_id.clone(),
        }],
    };
    let mut fields = vec![
        GeoRequestedField {
            field_id: "native_id".to_string(),
            role: GeoFieldRole::Identifier,
            required: true,
        },
        GeoRequestedField {
            field_id: evidence_class_field(gap.requested_evidence_class).to_string(),
            role: if matches!(
                gap.requested_evidence_class,
                GeoEvidenceClass::GeocodePoint
                    | GeoEvidenceClass::ParcelGeometry
                    | GeoEvidenceClass::BuildingFootprint
            ) {
                GeoFieldRole::Geometry
            } else {
                GeoFieldRole::Attribute
            },
            required: true,
        },
        GeoRequestedField {
            field_id: "source_record_digest".to_string(),
            role: GeoFieldRole::Digest,
            required: true,
        },
    ];
    fields.sort();
    fields.dedup();
    let probe_fields = fields
        .iter()
        .map(|field| field.field_id.clone())
        .collect::<Vec<_>>();
    let ceilings = GeoRowByteCeilings {
        max_rows,
        max_bytes,
    };
    let mut request = GeoDiscoveryRequest {
        version: CANON_GEO_DISCOVERY_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        bounded_geography: question.bounded_geography.clone(),
        subset: subset.clone(),
        requested_entity_levels: vec![entity_level],
        requested_evidence_classes: vec![gap.requested_evidence_class],
        release_selection: GeoDiscoveryReleaseSelectionPolicy {
            as_of_utc_day: as_of.utc_day.clone(),
            mode: GeoReleaseSelectionMode::LatestNotAfterAsOf,
            candidate_release_ids: Vec::new(),
        },
        releases: Vec::new(),
        fields,
        required_steps: vec![
            GeoDiscoveryStep::CatalogSearch,
            GeoDiscoveryStep::ListReleases,
            GeoDiscoveryStep::DescribeSchema,
            GeoDiscoveryStep::ColumnReadabilityProbe,
        ],
        column_readability_probe: GeoColumnReadabilityProbe {
            probe_id: format!("probe.{}", gap.gap_id),
            fields: probe_fields,
            subset,
            ceilings: ceilings.clone(),
        },
        ceilings,
    };
    request.request_id = geo_discovery_request_id(&request).map_err(discovery_error)?;
    validate_geo_discovery_request(&request).map_err(discovery_error)?;
    Ok(Some(request))
}

fn digest_contract(id: &str, value: &str) -> Result<GeoDigest, GeoPlanError> {
    let Some(hex_digest) = value.strip_prefix("blake3:") else {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo planning expected a canonical blake3 digest",
            [("digest", value)],
        ));
    };
    Ok(GeoDigest {
        digest_id: id.to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: hex_digest.to_string(),
    })
}

fn grain_project_stages(
    prefix: &str,
    level: GeoControlEntityLevel,
    evidence_classes: Vec<GeoEvidenceClass>,
    claim_classes: Vec<super::GeoClaimClass>,
    input_refs: Vec<ProjectPlanHashRef>,
    deterministic_bounds: &[GeoNumericBound],
    limits: &BTreeMap<String, u64>,
) -> Vec<(ProjectExtensionDagNode, GeoPlanNodeOverlay)> {
    let stages = [
        (
            "home_cells",
            ProjectPlanNodeKind::Normalize,
            HOME_CELLS_COMMAND,
            "canon_geo_home_cell_assignment.v1",
            GeoPlanStage::MaterializeHomeCells,
        ),
        (
            "section",
            ProjectPlanNodeKind::Block,
            TILE_WORK_COMMAND,
            "canon_geo_tile_work_unit.v1",
            GeoPlanStage::BuildBoundedSection,
        ),
        (
            "materialize_evidence",
            ProjectPlanNodeKind::Evidence,
            MATERIALIZE_EVIDENCE_COMMAND,
            "canon_geo_evidence_request.v0",
            GeoPlanStage::MaterializeEvidence,
        ),
        (
            "compile_evidence",
            ProjectPlanNodeKind::Evidence,
            COMPILE_EVIDENCE_COMMAND,
            "canon_geo_evidence_compilation.v0",
            GeoPlanStage::CompileEvidence,
        ),
        (
            "propagate",
            ProjectPlanNodeKind::Solve,
            PROPAGATE_COMMAND,
            CANON_GEO_PROPAGATION_VERSION,
            GeoPlanStage::PropagateConstraints,
        ),
        (
            "solve",
            ProjectPlanNodeKind::Solve,
            SOLVE_COMMAND,
            "canon_geo_composition.v0",
            GeoPlanStage::FactorAndSolveExactResidual,
        ),
    ];
    let mut result = Vec::new();
    let mut dependency = None;
    for (suffix, kind, command, contract, stage) in stages {
        let node_id = format!("{prefix}.{suffix}");
        let dependencies = if matches!(stage, GeoPlanStage::FactorAndSolveExactResidual) {
            vec![
                format!("{prefix}.compile_evidence"),
                format!("{prefix}.propagate"),
                format!("{prefix}.section"),
            ]
        } else {
            dependency.iter().cloned().collect()
        };
        let content_hash_inputs = if dependency.is_none() {
            input_refs.clone()
        } else {
            Vec::new()
        };
        let output_id = if matches!(stage, GeoPlanStage::PropagateConstraints) {
            GEO_PROPAGATE_OUTPUT_ID
        } else {
            suffix
        };
        let node = project_node(
            &node_id,
            kind,
            command,
            dependencies,
            content_hash_inputs,
            output_id,
            &format!("geo/{}/{output_id}.json", level_name(level)),
            limits.clone(),
        );
        let preconditions = match stage {
            GeoPlanStage::MaterializeHomeCells => vec![precondition(
                GeoPlanGatePlane::Availability,
                GeoPlanGateStatus::SatisfiedByDeclaredInput,
                "all profile-required evidence classes have local inventory artifacts",
            )],
            GeoPlanStage::BuildBoundedSection => vec![
                precondition(
                    GeoPlanGatePlane::Coverage,
                    GeoPlanGateStatus::PendingArtifact,
                    "tile-work must bind one center plus a controlled halo; H3 is blocking and ownership metadata, not geometric truth",
                ),
                precondition(
                    GeoPlanGatePlane::CandidateReach,
                    GeoPlanGateStatus::UnverifiedWithClaimLimitation,
                    "truth reach is not inferred from source availability; solving may be exact only relative to the declared candidate universe",
                ),
            ],
            GeoPlanStage::MaterializeEvidence | GeoPlanStage::CompileEvidence => {
                vec![precondition(
                    GeoPlanGatePlane::Admission,
                    GeoPlanGateStatus::PendingArtifact,
                    "every restricting observation requires a versioned rho admission",
                )]
            }
            GeoPlanStage::PropagateConstraints => vec![
                precondition(
                    GeoPlanGatePlane::ConstraintEffect,
                    GeoPlanGateStatus::PendingArtifact,
                    "sound typed propagators prune only values entailed by admitted hard constraints",
                ),
                precondition(
                    GeoPlanGatePlane::SolverCorrectness,
                    GeoPlanGateStatus::PendingArtifact,
                    "propagation must pass black-box residual soundness before the exact solve consumes it",
                ),
                precondition(
                    GeoPlanGatePlane::Cost,
                    GeoPlanGateStatus::SatisfiedByDeclaredInput,
                    "propagation fallback is controlled by deterministic fixpoint, Hall-set, and subset-sum counters",
                ),
            ],
            GeoPlanStage::FactorAndSolveExactResidual => vec![
                precondition(
                    GeoPlanGatePlane::Coverage,
                    GeoPlanGateStatus::StructurallyCompleteRelativeToInputs,
                    "the solve consumes the declared bounded section and compiled evidence artifacts",
                ),
                precondition(
                    GeoPlanGatePlane::CandidateReach,
                    GeoPlanGateStatus::UnverifiedWithClaimLimitation,
                    "candidate reach is reported independently and is not repaired by the solver",
                ),
                precondition(
                    GeoPlanGatePlane::SolverCorrectness,
                    GeoPlanGateStatus::PendingArtifact,
                    "canon geo solve builds the actual incidence graph and solves its bounded components under deterministic counters",
                ),
                precondition(
                    GeoPlanGatePlane::Cost,
                    GeoPlanGateStatus::SatisfiedByDeclaredInput,
                    "only deterministic row/byte/candidate/variable/state/model/operation/proof counters may control fallback",
                ),
            ],
        };
        let exact_solve_scope =
            matches!(stage, GeoPlanStage::FactorAndSolveExactResidual).then(|| {
                GeoPlanExactSolveScope {
                bounded_section: GeoPlanProducedArtifactRef {
                    producer_node_id: format!("{prefix}.section"),
                    output_id: "section".to_string(),
                    output_contract: CANON_GEO_TILE_WORK_UNIT_VERSION.to_string(),
                },
                evidence_compilation: GeoPlanProducedArtifactRef {
                    producer_node_id: format!("{prefix}.compile_evidence"),
                    output_id: "compile_evidence".to_string(),
                    output_contract: CANON_GEO_EVIDENCE_COMPILATION_VERSION.to_string(),
                },
                component_scope:
                    GeoPlanComponentScope::ActualConnectedComponentsOfCompiledConstraintIncidence,
                component_key_field: "canon_geo_composition.v0.factorization[].key".to_string(),
            }
            });
        let overlay = overlay_node(
            &node_id,
            stage,
            Some(level),
            evidence_classes.clone(),
            claim_classes.clone(),
            contract,
            preconditions,
            GeoPlanClaimEffect::CanChangeRequestedClaim,
            matches!(stage, GeoPlanStage::FactorAndSolveExactResidual),
            matches!(stage, GeoPlanStage::FactorAndSolveExactResidual),
            exact_solve_scope,
            deterministic_bounds.to_vec(),
        );
        dependency = Some(node_id);
        result.push((node, overlay));
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn project_node(
    node_id: &str,
    kind: ProjectPlanNodeKind,
    command: &str,
    dependencies: Vec<String>,
    content_hash_inputs: Vec<ProjectPlanHashRef>,
    output_id: &str,
    output_path: &str,
    limits: BTreeMap<String, u64>,
) -> ProjectExtensionDagNode {
    ProjectExtensionDagNode {
        node_id: node_id.to_string(),
        kind,
        class: ProjectPlanNodeClass::Computation,
        command: command.to_string(),
        dependencies,
        content_hash_inputs,
        outputs: vec![ProjectExtensionDagOutput {
            output_id: output_id.to_string(),
            path: output_path.to_string(),
            materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
        }],
        limits,
        cache_eligible: true,
        side_effects: vec![
            ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::ReadsInput,
                description: "reads only declared local typed inputs".to_string(),
            },
            ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::WritesArtifact,
                description: "publishes one declared content-addressed artifact".to_string(),
            },
        ],
        refusal_conditions: vec![ProjectPlanRefusalCondition {
            code: ProjectPlanErrorCode::ArtifactContract,
            message: "refuse when an input or output violates its declared contract".to_string(),
            next_command: None,
        }],
    }
}

#[allow(clippy::too_many_arguments)]
fn overlay_node(
    project_node_id: &str,
    stage: GeoPlanStage,
    entity_level: Option<GeoControlEntityLevel>,
    mut evidence_classes: Vec<GeoEvidenceClass>,
    mut claim_classes: Vec<super::GeoClaimClass>,
    expected_output_contract: &str,
    mut preconditions: Vec<GeoPlanPrecondition>,
    claim_effect: GeoPlanClaimEffect,
    bounded_section_required: bool,
    incidence_factorization_required: bool,
    exact_solve_scope: Option<GeoPlanExactSolveScope>,
    mut deterministic_bounds: Vec<GeoNumericBound>,
) -> GeoPlanNodeOverlay {
    evidence_classes.sort();
    evidence_classes.dedup();
    claim_classes.sort();
    claim_classes.dedup();
    preconditions.sort();
    preconditions.dedup();
    deterministic_bounds.sort();
    deterministic_bounds.dedup();
    let cost_estimate_ranges = deterministic_bounds
        .iter()
        .map(|bound| GeoPlanCostEstimateRange {
            semantic_id: format!("estimate.{}", bound.semantic_id),
            counter: bound.counter,
            lower_bound: 0,
            upper_bound: bound.value,
            unit: bound.unit.clone(),
            basis: "no calibrated stage estimate is declared; range is bounded only by the deterministic ceiling".to_string(),
            semantic_effect: GeoTelemetrySemanticEffect::None,
        })
        .collect();
    GeoPlanNodeOverlay {
        project_node_id: project_node_id.to_string(),
        stage,
        entity_level,
        evidence_classes,
        claim_classes,
        expected_output_contract: expected_output_contract.to_string(),
        preconditions,
        claim_effect,
        bounded_section_required,
        incidence_factorization_required,
        exact_solve_scope,
        deterministic_bounds,
        cost_estimate_ranges,
        transitions: GeoPlanTransitionSet {
            success: "validate output and unlock declared dependents".to_string(),
            abstention: "preserve completed artifacts; stop this grain before solve on failed coverage or reference reach; otherwise report the typed residual".to_string(),
            contradiction: "preserve the empty residual and diagnose admitted evidence".to_string(),
            budget_fallback: "apply each deterministic bound's declared action; preserve completed components and emit BudgetFallback only for report_budget_fallback; telemetry never selects a transition".to_string(),
        },
    }
}

fn precondition(
    plane: GeoPlanGatePlane,
    status: GeoPlanGateStatus,
    detail: &str,
) -> GeoPlanPrecondition {
    GeoPlanPrecondition {
        plane,
        status,
        detail: detail.to_string(),
    }
}

fn project_limits(budget: &GeoResourceBudget) -> BTreeMap<String, u64> {
    budget
        .deterministic_bounds
        .iter()
        .map(|bound| (bound.semantic_id.clone(), bound.value))
        .collect()
}

fn budget_limit(budget: &GeoResourceBudget, counter: GeoResourceCounter) -> Option<u64> {
    budget
        .deterministic_bounds
        .iter()
        .filter(|bound| bound.counter == counter)
        .map(|bound| bound.value)
        .min()
}

fn validate_supported_grain_budget(budget: &GeoResourceBudget) -> Result<(), GeoPlanError> {
    let required = [
        GeoResourceCounter::Bytes,
        GeoResourceCounter::Rows,
        GeoResourceCounter::Cells,
        GeoResourceCounter::Candidates,
        GeoResourceCounter::Variables,
        GeoResourceCounter::States,
        GeoResourceCounter::Models,
        GeoResourceCounter::Operations,
    ];
    let missing = required
        .into_iter()
        .filter(|counter| budget_limit(budget, *counter).is_none_or(|limit| limit == 0))
        .map(|counter| format!("{counter:?}").to_lowercase())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    Err(GeoPlanError::new(
        GeoPlanErrorCode::InvalidInput,
        "supported Geo grains require positive deterministic ceilings for bounded sectioning and exact residual solving",
        [("missing_counters", missing.join(","))],
    ))
}

fn validate_overlay_bijection(
    project_plan: &ProjectPlan,
    geo_nodes: &[GeoPlanNodeOverlay],
) -> Result<(), GeoPlanError> {
    let project_ids = project_plan
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<BTreeSet<_>>();
    let geo_ids = geo_nodes
        .iter()
        .map(|node| node.project_node_id.as_str())
        .collect::<BTreeSet<_>>();
    if project_ids != geo_ids || geo_ids.len() != geo_nodes.len() {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "Geo plan nodes must map one-to-one onto the embedded project plan nodes",
            [
                ("project_nodes", project_ids.len().to_string()),
                ("geo_nodes", geo_nodes.len().to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_node_cost_contract(
    plan: &GeoPlan,
    overlay: &GeoPlanNodeOverlay,
) -> Result<(), GeoPlanError> {
    let project_node = plan
        .project_plan
        .nodes
        .iter()
        .find(|node| node.node_id == overlay.project_node_id)
        .expect("overlay bijection validated project node existence");
    let expected_limits = overlay
        .deterministic_bounds
        .iter()
        .map(|bound| (bound.semantic_id.clone(), bound.value))
        .collect::<BTreeMap<_, _>>();
    if project_node.limits != expected_limits {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "project-node limits must preserve every Geo deterministic bound by semantic id",
            [("project_node_id", overlay.project_node_id.as_str())],
        ));
    }
    if overlay.cost_estimate_ranges.len() != overlay.deterministic_bounds.len() {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "every deterministic bound requires one explicit non-semantic estimate range",
            [("project_node_id", overlay.project_node_id.as_str())],
        ));
    }
    for bound in &overlay.deterministic_bounds {
        let expected_id = format!("estimate.{}", bound.semantic_id);
        let Some(estimate) = overlay
            .cost_estimate_ranges
            .iter()
            .find(|estimate| estimate.semantic_id == expected_id)
        else {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "deterministic bound is missing its estimate range",
                [
                    ("project_node_id", overlay.project_node_id.as_str()),
                    ("bound", bound.semantic_id.as_str()),
                ],
            ));
        };
        if estimate.counter != bound.counter
            || estimate.lower_bound > estimate.upper_bound
            || estimate.upper_bound != bound.value
            || estimate.unit != bound.unit
            || estimate.semantic_effect != GeoTelemetrySemanticEffect::None
            || estimate.basis.trim().is_empty()
        {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "cost estimates must be bounded, labeled telemetry and cannot control fallback",
                [
                    ("project_node_id", overlay.project_node_id.as_str()),
                    ("estimate", estimate.semantic_id.as_str()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_solve_scope(
    plan: &GeoPlan,
    solve: &GeoPlanNodeOverlay,
    scope: &GeoPlanExactSolveScope,
) -> Result<(), GeoPlanError> {
    let solve_project_node = plan
        .project_plan
        .nodes
        .iter()
        .find(|node| node.node_id == solve.project_node_id)
        .expect("overlay bijection validated solve node existence");
    if solve_project_node.kind != ProjectPlanNodeKind::Solve
        || solve_project_node.command != SOLVE_COMMAND
        || solve.expected_output_contract != CANON_GEO_COMPOSITION_VERSION
        || scope.component_key_field != "canon_geo_composition.v0.factorization[].key"
    {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "exact-solve overlay must bind the implemented Geo solve leaf and composition contract",
            [("project_node_id", solve.project_node_id.as_str())],
        ));
    }
    validate_produced_artifact_ref(
        plan,
        &scope.bounded_section,
        GeoPlanStage::BuildBoundedSection,
        CANON_GEO_TILE_WORK_UNIT_VERSION,
    )?;
    validate_produced_artifact_ref(
        plan,
        &scope.evidence_compilation,
        GeoPlanStage::CompileEvidence,
        CANON_GEO_EVIDENCE_COMPILATION_VERSION,
    )?;
    for producer in [
        &scope.bounded_section.producer_node_id,
        &scope.evidence_compilation.producer_node_id,
    ] {
        if !solve_project_node
            .dependencies
            .iter()
            .any(|dependency| dependency == producer)
        {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "exact solve scope artifacts must be declared direct dependencies of the solve node",
                [
                    ("project_node_id", solve.project_node_id.as_str()),
                    ("producer_node_id", producer.as_str()),
                ],
            ));
        }
        if !project_node_is_ancestor(&plan.project_plan, producer, &solve.project_node_id) {
            return Err(GeoPlanError::new(
                GeoPlanErrorCode::ContractViolation,
                "exact solve scope must reference upstream artifacts in the same project DAG",
                [
                    ("project_node_id", solve.project_node_id.as_str()),
                    ("producer_node_id", producer.as_str()),
                ],
            ));
        }
    }
    let reach = solve
        .preconditions
        .iter()
        .filter(|precondition| precondition.plane == GeoPlanGatePlane::CandidateReach)
        .collect::<Vec<_>>();
    if reach.len() != 1
        || !matches!(
            reach[0].status,
            GeoPlanGateStatus::PassedAgainstReference
                | GeoPlanGateStatus::UnverifiedWithClaimLimitation
        )
    {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "exact solve requires one explicit passed or claim-limited-unverified reach state",
            [("project_node_id", solve.project_node_id.as_str())],
        ));
    }
    Ok(())
}

fn validate_produced_artifact_ref(
    plan: &GeoPlan,
    artifact: &GeoPlanProducedArtifactRef,
    expected_stage: GeoPlanStage,
    expected_contract: &str,
) -> Result<(), GeoPlanError> {
    let Some(project_node) = plan
        .project_plan
        .nodes
        .iter()
        .find(|node| node.node_id == artifact.producer_node_id)
    else {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "exact solve scope references a missing producer node",
            [("producer_node_id", artifact.producer_node_id.as_str())],
        ));
    };
    if !project_node
        .outputs
        .iter()
        .any(|output| output.output_id == artifact.output_id)
    {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "exact solve scope references a missing producer output",
            [
                ("producer_node_id", artifact.producer_node_id.as_str()),
                ("output_id", artifact.output_id.as_str()),
            ],
        ));
    }
    let producer_overlay = plan
        .geo_nodes
        .iter()
        .find(|node| node.project_node_id == artifact.producer_node_id)
        .expect("overlay bijection validated producer overlay existence");
    if producer_overlay.stage != expected_stage
        || producer_overlay.expected_output_contract != expected_contract
        || artifact.output_contract != expected_contract
    {
        return Err(GeoPlanError::new(
            GeoPlanErrorCode::ContractViolation,
            "exact solve scope producer stage or output contract does not match",
            [("producer_node_id", artifact.producer_node_id.as_str())],
        ));
    }
    Ok(())
}

fn project_node_is_ancestor(plan: &ProjectPlan, ancestor: &str, descendant: &str) -> bool {
    let by_id = plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut pending = vec![descendant];
    let mut seen = BTreeSet::new();
    while let Some(node_id) = pending.pop() {
        if !seen.insert(node_id) {
            continue;
        }
        let Some(node) = by_id.get(node_id) else {
            continue;
        };
        for dependency in &node.dependencies {
            if dependency == ancestor {
                return true;
            }
            pending.push(dependency);
        }
    }
    false
}

fn external_request_sort_key(request: &GeoPlanExternalRequest) -> String {
    match request {
        GeoPlanExternalRequest::Acquisition { request, .. } => {
            format!("0:{}", request.request_id)
        }
        GeoPlanExternalRequest::Discovery { gap_id, request } => {
            format!("1:{gap_id}:{}", request.request_id)
        }
        GeoPlanExternalRequest::DiscoveryGap { gap } => format!("2:{}", gap.gap_id),
    }
}

fn canonicalize_plan_external_request(request: &mut GeoPlanExternalRequest) {
    match request {
        GeoPlanExternalRequest::Acquisition { request, .. } => {
            let request_id = request.request_id.clone();
            *request = canonicalize_geo_acquisition_request(request);
            request.request_id = request_id;
        }
        GeoPlanExternalRequest::Discovery { request, .. } => {
            let request_id = request.request_id.clone();
            *request = canonicalize_geo_discovery_request(request);
            request.request_id = request_id;
        }
        GeoPlanExternalRequest::DiscoveryGap { .. } => {}
    }
}

fn canonicalize_replan_subset(subset: &GeoBoundedSubset) -> GeoBoundedSubset {
    let mut canonical = subset.clone();
    canonical.h3_cells.sort();
    canonical.h3_cells.dedup();
    canonical.predicates.sort();
    canonical.predicates.dedup();
    canonical
}

fn evidence_class_field(class: GeoEvidenceClass) -> &'static str {
    match class {
        GeoEvidenceClass::GeocodePoint => "geocode_point",
        GeoEvidenceClass::AddressString => "address_string",
        GeoEvidenceClass::AddressSet => "address_set",
        GeoEvidenceClass::ParcelGeometry => "parcel_geometry",
        GeoEvidenceClass::BuildingFootprint => "building_footprint",
        GeoEvidenceClass::AssertedAttribute => "asserted_attribute",
        GeoEvidenceClass::EntityRelation => "entity_relation",
        GeoEvidenceClass::TemporalObservation => "temporal_observation",
    }
}

fn level_name(level: GeoControlEntityLevel) -> &'static str {
    match level {
        GeoControlEntityLevel::Site => "site",
        GeoControlEntityLevel::Property => "property",
        GeoControlEntityLevel::Parcel => "parcel",
        GeoControlEntityLevel::Building => "building",
        GeoControlEntityLevel::Unit => "unit",
        GeoControlEntityLevel::Address => "address",
        GeoControlEntityLevel::Poi => "poi",
    }
}

fn hash_ref(id: &str, content_hash: &str) -> ProjectPlanHashRef {
    ProjectPlanHashRef {
        ref_id: id.to_string(),
        content_hash: content_hash.to_string(),
    }
}

fn digest_json(value: &impl Serialize) -> Result<String, GeoPlanError> {
    serde_json::to_vec(value)
        .map(|bytes| format!("blake3:{}", blake3::hash(&bytes).to_hex()))
        .map_err(serialization_error)
}

fn control_error(error: super::GeoControlError) -> GeoPlanError {
    GeoPlanError::new(
        match error.code {
            super::GeoControlErrorCode::UnsupportedVersion => GeoPlanErrorCode::UnsupportedVersion,
            _ => GeoPlanErrorCode::InvalidInput,
        },
        error.message,
        error.detail,
    )
}

fn composition_profile_error(error: super::GeoCompositionError) -> GeoPlanError {
    GeoPlanError::new(
        match error.code {
            super::GeoCompositionErrorCode::UnsupportedVersion => {
                GeoPlanErrorCode::UnsupportedVersion
            }
            _ => GeoPlanErrorCode::ContractViolation,
        },
        error.message,
        error.detail,
    )
}

fn discovery_error(error: super::GeoDiscoveryError) -> GeoPlanError {
    GeoPlanError::new(
        GeoPlanErrorCode::ContractViolation,
        error.message,
        error.detail,
    )
}

fn satisfy_error(error: GeoSatisfyError) -> GeoPlanError {
    GeoPlanError::new(
        GeoPlanErrorCode::ContractViolation,
        error.message,
        error.detail,
    )
}

fn project_error(error: crate::project::ProjectPlanError) -> GeoPlanError {
    GeoPlanError::new(
        GeoPlanErrorCode::ContractViolation,
        error.message,
        [("project_error", format!("{:?}", error.code))],
    )
}

fn serialization_error(error: serde_json::Error) -> GeoPlanError {
    GeoPlanError::new(
        GeoPlanErrorCode::Serialization,
        "failed to serialize canonical Geo plan",
        [("error", error.to_string())],
    )
}
