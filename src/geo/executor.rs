#![forbid(unsafe_code)]

//! In-process executor for the Geo planner's offline leaf commands.
//!
//! The executor is deliberately a typed adapter over the existing Geo kernels.
//! It never shells out, opens files, reaches the network, or publishes
//! artifacts itself; publication and run receipts remain owned by the shared
//! project runner.

use crate::{
    geo::{
        CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION, CANON_GEO_COMPOSITION_REQUEST_VERSION,
        CANON_GEO_COMPOSITION_VERSION, CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_EXPLANATION_VERSION,
        CANON_GEO_GEOMETRY_TILE_VERSION, CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
        CANON_GEO_HOME_CELL_ROWS_VERSION, CANON_GEO_PROPAGATION_VERSION,
        CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION,
        CANON_GEO_WAREHOUSE_ROWS_VERSION, GeoClientTileIngestRequest, GeoCompositionArtifact,
        GeoCompositionStatus, GeoControlEntityLevel, GeoEntityLevel,
        GeoEvidenceCompilationArtifact, GeoEvidenceCompilationReference,
        GeoEvidenceCompilationRequest, GeoExplanationArtifact, GeoExplanationBudget,
        GeoGeometryTileArtifact, GeoHomeCellAssignmentArtifact, GeoHomeCellRowsRequest, GeoPlan,
        GeoPlanComponentScope, GeoPlanExactSolveScope, GeoPlanProducedArtifactRef,
        GeoPropagationArtifact, GeoPropagationBudget, GeoTileCandidateReachStatus,
        GeoTileWorkRequest, GeoTileWorkUnitArtifact, GeoWarehouseRowsRequest, apply_prunings,
        assessment_roll::{
            CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION,
            CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION, GeoAssessmentRollOwnerArtifact,
            GeoAssessmentRollOwnerRequest, canonical_assessment_roll_owner_bytes,
            produce_assessment_roll_owner_evidence, validate_assessment_roll_owner_artifact,
        },
        canonical_composition_bytes, canonical_evidence_compilation_bytes,
        canonical_explanation_bytes, canonical_geometry_tile_bytes,
        canonical_home_cell_assignment_bytes, canonical_materialized_evidence_request_bytes,
        canonical_propagation_bytes, canonical_tile_work_unit_bytes, compile_evidence,
        condo::{
            CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION, CANON_GEO_CONDO_BRIDGE_VERSION,
            GeoCondoBridgeArtifact, GeoCondoBridgeRequest, build_condo_bridge,
            canonical_condo_bridge_bytes, validate_condo_bridge_artifact,
        },
        correction_sets,
        footprint_roll::{
            CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION, GeoFootprintRollEvidenceRequest,
            materialize_footprint_roll_evidence,
        },
        ingest_client_geometry_tile, materialize_home_cells, materialize_tile_work_unit,
        materialize_warehouse_rows, minimal_core, propagate, reliability_order_from_evidence,
        solve_composition, validate_evidence_compilation_artifact, validate_explanation_artifact,
        validate_propagation_artifact,
    },
    project::{
        ProjectDependencyOutput, ProjectNodeExecutionContext, ProjectNodeExecutionResult,
        ProjectNodeExecutor, ProjectPlanNode, ProjectPlanNodeClass, ProjectPlanNodeKind,
        ProjectPlanOutputMaterialization, ProjectPlanSideEffectKind, ProjectRunError,
        ProjectRunErrorCode, ProjectRunResult,
    },
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const GEO_EXECUTOR_ID: &str = "canon.geo.project_node_executor.v0";
pub const GEO_EXECUTOR_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const GEO_MATERIALIZE_HOME_CELLS_COMMAND: &str =
    "canon geo materialize-home-cells --rows <ROWS.json>";
pub const GEO_TILE_WORK_COMMAND: &str = "canon geo tile-work --request <REQUEST.json>";
pub const GEO_MATERIALIZE_EVIDENCE_COMMAND: &str =
    "canon geo materialize-evidence --rows <ROWS.json>";
pub const GEO_COMPILE_EVIDENCE_COMMAND: &str =
    "canon geo compile-evidence --request <REQUEST.json>";
pub const GEO_PROPAGATE_STAGE_COMMAND: &str = "canon.geo.stage.propagate.v0";
pub const GEO_PROPAGATE_OUTPUT_ID: &str = "propagation";
pub const GEO_EXPLAIN_STAGE_COMMAND: &str = "canon.geo.stage.explain.v0";
pub const GEO_EXPLAIN_OUTPUT_ID: &str = "explanation";
pub const GEO_ASSESSMENT_ROLL_OWNER_STAGE_COMMAND: &str =
    "canon.geo.stage.assessment_roll_owner.v0";
pub const GEO_ASSESSMENT_ROLL_OWNER_OUTPUT_ID: &str = "assessment_roll_owner";
pub const GEO_CONDO_BRIDGE_STAGE_COMMAND: &str = "canon.geo.stage.condo_bridge.v0";
pub const GEO_CONDO_BRIDGE_OUTPUT_ID: &str = "condo_bridge";
pub const GEO_FOOTPRINT_ROLL_EVIDENCE_STAGE_COMMAND: &str =
    "canon.geo.stage.footprint_roll_evidence.v0";
pub const GEO_FOOTPRINT_ROLL_EVIDENCE_OUTPUT_ID: &str = "footprint_roll_evidence";
pub const GEO_SOLVE_COMMAND: &str = "canon geo solve --request <REQUEST.json>";
pub const GEO_CLIENT_TILE_INGEST_STAGE_COMMAND: &str = "canon.geo.stage.client_tile_ingest.v0";
pub const CANON_GEO_CLIENT_TILE_SOURCE_VERSION: &str = "canon_geo_client_tile_source.v0";

pub const GEO_ROWS_BINDING_ID: &str = "rows";
pub const GEO_REQUEST_BINDING_ID: &str = "request";
pub const GEO_CLIENT_TILE_SOURCE_BINDING_ID: &str = "source";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoExecutorInputBinding {
    pub node_id: String,
    pub binding_id: String,
    pub contract: String,
    pub content_hash: String,
    pub bytes: Vec<u8>,
}

impl GeoExecutorInputBinding {
    pub fn from_json<T: Serialize>(
        node_id: impl Into<String>,
        binding_id: impl Into<String>,
        contract: impl Into<String>,
        value: &T,
    ) -> Result<Self, serde_json::Error> {
        let bytes = serde_json::to_vec(value)?;
        Ok(Self::from_bytes(node_id, binding_id, contract, bytes))
    }

    pub fn from_bytes(
        node_id: impl Into<String>,
        binding_id: impl Into<String>,
        contract: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Self {
        let content_hash = geo_executor_content_hash(&bytes);
        Self {
            node_id: node_id.into(),
            binding_id: binding_id.into(),
            contract: contract.into(),
            content_hash,
            bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoExecutorDependencyOutput {
    pub producer_node_id: String,
    pub output_id: String,
    pub contract: String,
    pub content_hash: String,
    pub bytes: Vec<u8>,
}

impl GeoExecutorDependencyOutput {
    pub fn from_json<T: Serialize>(
        producer_node_id: impl Into<String>,
        output_id: impl Into<String>,
        contract: impl Into<String>,
        value: &T,
    ) -> Result<Self, serde_json::Error> {
        let bytes = serde_json::to_vec(value)?;
        Ok(Self::from_bytes(
            producer_node_id,
            output_id,
            contract,
            bytes,
        ))
    }

    pub fn from_bytes(
        producer_node_id: impl Into<String>,
        output_id: impl Into<String>,
        contract: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Self {
        let content_hash = geo_executor_content_hash(&bytes);
        Self {
            producer_node_id: producer_node_id.into(),
            output_id: output_id.into(),
            contract: contract.into(),
            content_hash,
            bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerifiedGeoArtifact {
    producer_node_id: String,
    output_id: String,
    contract: String,
    content_hash: String,
    bytes: Vec<u8>,
}

struct GeoLeafExecution {
    output_id: &'static str,
    output_contract: &'static str,
    output_bytes: Vec<u8>,
    deterministic_usage: BTreeMap<String, u64>,
}

pub struct GeoProjectNodeExecutor {
    input_bindings: BTreeMap<(String, String), GeoExecutorInputBinding>,
    dependency_outputs: BTreeMap<(String, String), VerifiedGeoArtifact>,
    exact_solve_scopes: BTreeMap<String, GeoPlanExactSolveScope>,
}

impl GeoProjectNodeExecutor {
    pub fn new() -> Self {
        Self {
            input_bindings: BTreeMap::new(),
            dependency_outputs: BTreeMap::new(),
            exact_solve_scopes: BTreeMap::new(),
        }
    }

    pub fn bind_geo_plan(&mut self, plan: &GeoPlan) {
        self.dependency_outputs.clear();
        self.exact_solve_scopes = plan
            .geo_nodes
            .iter()
            .filter_map(|overlay| {
                overlay
                    .exact_solve_scope
                    .clone()
                    .map(|scope| (overlay.project_node_id.clone(), scope))
            })
            .collect();
    }

    pub fn with_exact_solve_scope(
        mut self,
        solve_node_id: impl Into<String>,
        scope: GeoPlanExactSolveScope,
    ) -> Self {
        self.exact_solve_scopes.insert(solve_node_id.into(), scope);
        self
    }

    pub fn with_input_binding(mut self, binding: GeoExecutorInputBinding) -> Self {
        self.insert_input_binding(binding);
        self
    }

    pub fn insert_input_binding(&mut self, binding: GeoExecutorInputBinding) {
        self.input_bindings.insert(
            (binding.node_id.clone(), binding.binding_id.clone()),
            binding,
        );
    }

    pub fn insert_dependency_output(
        &mut self,
        output: GeoExecutorDependencyOutput,
    ) -> ProjectRunResult<()> {
        verify_digest(
            &output.content_hash,
            &output.bytes,
            &output.producer_node_id,
            "dependency_output.content_hash",
        )?;
        ensure_canonical_artifact_bytes(&output.producer_node_id, &output.contract, &output.bytes)?;
        let artifact = VerifiedGeoArtifact {
            producer_node_id: output.producer_node_id,
            output_id: output.output_id,
            contract: output.contract,
            content_hash: output.content_hash,
            bytes: output.bytes,
        };
        self.dependency_outputs.insert(
            (
                artifact.producer_node_id.clone(),
                artifact.output_id.clone(),
            ),
            artifact,
        );
        Ok(())
    }

    fn execute_typed(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> ProjectRunResult<ProjectNodeExecutionResult> {
        if context.node_id != node.node_id {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                "project executor context node_id does not match the node being executed",
            ));
        }
        let command = GeoExecutorCommand::from_command(node)?;
        validate_node_contract(node, context, command)?;
        self.clear_dependency_outputs_for_declared_producers(node);
        self.ingest_context_dependency_outputs(node, context)?;
        validate_expected_dependency(node, command, &self.dependency_outputs)?;
        self.validate_no_forbidden_direct_bindings(node, command)?;

        let leaf = match command {
            GeoExecutorCommand::MaterializeHomeCells => self.execute_home_cells(node)?,
            GeoExecutorCommand::TileWork => self.execute_tile_work(node)?,
            GeoExecutorCommand::ClientTileIngest => self.execute_client_tile_ingest(node)?,
            GeoExecutorCommand::MaterializeEvidence => self.execute_materialize_evidence(node)?,
            GeoExecutorCommand::CompileEvidence => self.execute_compile_evidence(node)?,
            GeoExecutorCommand::AssessmentRollOwner => self.execute_assessment_roll_owner(node)?,
            GeoExecutorCommand::CondoBridge => self.execute_condo_bridge(node)?,
            GeoExecutorCommand::FootprintRollEvidence => {
                self.execute_footprint_roll_evidence(node)?
            }
            GeoExecutorCommand::Propagate => self.execute_propagate(node)?,
            GeoExecutorCommand::Explain => self.execute_explain(node)?,
            GeoExecutorCommand::Solve => self.execute_solve(node)?,
        };
        ensure_canonical_artifact_bytes(node, leaf.output_contract, &leaf.output_bytes)?;
        self.remember_output(
            node,
            leaf.output_id,
            leaf.output_contract,
            leaf.output_bytes.clone(),
        );

        let mut outputs = BTreeMap::new();
        outputs.insert(leaf.output_id.to_string(), leaf.output_bytes);
        let mut result = ProjectNodeExecutionResult::with_outputs(outputs);
        result.deterministic_usage = leaf.deterministic_usage;
        result.deterministic_usage.insert(
            "dependency_output_count".to_string(),
            node.dependencies.len() as u64,
        );
        result.deterministic_usage.insert(
            "input_binding_count".to_string(),
            self.input_bindings
                .keys()
                .filter(|(node_id, _)| node_id == &node.node_id)
                .count() as u64,
        );
        result.deterministic_usage.insert(
            "executor_supported_command_count".to_string(),
            GeoExecutorCommand::SUPPORTED.len() as u64,
        );
        Ok(result)
    }

    fn execute_home_cells(&self, node: &ProjectPlanNode) -> ProjectRunResult<GeoLeafExecution> {
        let request: GeoHomeCellRowsRequest = self.required_binding_json(
            node,
            GEO_ROWS_BINDING_ID,
            &[CANON_GEO_HOME_CELL_ROWS_VERSION],
        )?;
        let artifact = materialize_home_cells(&request)
            .map_err(|error| leaf_error(node, "materialize-home-cells", error))?;
        let bytes = canonical_home_cell_assignment_bytes(&artifact).map_err(|error| {
            serialization_error(node, CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, error)
        })?;
        let mut usage = BTreeMap::new();
        usage.insert("home_cell_rows".to_string(), request.rows.len() as u64);
        usage.insert(
            "home_cell_features".to_string(),
            artifact.features.len() as u64,
        );
        usage.insert(
            "boundary_sensitive_features".to_string(),
            artifact.summary.boundary_sensitive,
        );
        Ok(GeoLeafExecution {
            output_id: "home_cells",
            output_contract: CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn clear_dependency_outputs_for_declared_producers(&mut self, node: &ProjectPlanNode) {
        if node.dependencies.is_empty() {
            return;
        }
        let declared = node.dependencies.iter().collect::<BTreeSet<_>>();
        self.dependency_outputs
            .retain(|(producer_node_id, _), _| !declared.contains(producer_node_id));
    }

    fn ingest_context_dependency_outputs(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> ProjectRunResult<()> {
        let declared = node.dependencies.iter().collect::<BTreeSet<_>>();
        for (producer_node_id, outputs) in &context.dependency_outputs {
            if !declared.contains(producer_node_id) {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "Geo executor received dependency outputs from undeclared producer {producer_node_id}"
                    ),
                ));
            }
            for output in outputs {
                let contract = contract_for_output_id(&output.output_id).ok_or_else(|| {
                    error(
                        node,
                        ProjectRunErrorCode::ArtifactContract,
                        format!(
                            "Geo executor cannot infer a typed contract for dependency output {}:{}",
                            producer_node_id, output.output_id
                        ),
                    )
                })?;
                verify_project_dependency_output(node, producer_node_id, output)?;
                ensure_canonical_artifact_bytes(node, contract, &output.bytes)?;
                let artifact = VerifiedGeoArtifact {
                    producer_node_id: producer_node_id.clone(),
                    output_id: output.output_id.clone(),
                    contract: contract.to_string(),
                    content_hash: output.content_digest.clone(),
                    bytes: output.bytes.clone(),
                };
                self.dependency_outputs.insert(
                    (
                        artifact.producer_node_id.clone(),
                        artifact.output_id.clone(),
                    ),
                    artifact,
                );
            }
        }
        Ok(())
    }

    fn execute_tile_work(&self, node: &ProjectPlanNode) -> ProjectRunResult<GeoLeafExecution> {
        let request: GeoTileWorkRequest = self.required_binding_json(
            node,
            GEO_REQUEST_BINDING_ID,
            &[CANON_GEO_TILE_WORK_REQUEST_VERSION],
        )?;
        self.validate_tile_request_against_home_cells(node, &request)?;
        let artifact = materialize_tile_work_unit(&request)
            .map_err(|error| leaf_error(node, "tile-work", error))?;
        let bytes = canonical_tile_work_unit_bytes(&artifact)
            .map_err(|error| serialization_error(node, CANON_GEO_TILE_WORK_UNIT_VERSION, error))?;
        let mut usage = BTreeMap::new();
        usage.insert("tile_features".to_string(), artifact.features.len() as u64);
        usage.insert(
            "tile_work_cells".to_string(),
            artifact.work_cells.len() as u64,
        );
        usage.insert(
            "tile_center_features".to_string(),
            artifact.center_feature_count,
        );
        usage.insert(
            "tile_halo_features".to_string(),
            artifact.halo_feature_count,
        );
        Ok(GeoLeafExecution {
            output_id: "section",
            output_contract: CANON_GEO_TILE_WORK_UNIT_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_client_tile_ingest(
        &self,
        node: &ProjectPlanNode,
    ) -> ProjectRunResult<GeoLeafExecution> {
        let request: GeoClientTileIngestRequest = self.required_binding_json(
            node,
            GEO_REQUEST_BINDING_ID,
            &[CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION],
        )?;
        let source = self.required_binding(
            node,
            GEO_CLIENT_TILE_SOURCE_BINDING_ID,
            &[CANON_GEO_CLIENT_TILE_SOURCE_VERSION],
        )?;
        let artifact = ingest_client_geometry_tile(&request, &source.bytes)
            .map_err(|error| leaf_error(node, "client-tile-ingest", error))?;
        let bytes = canonical_geometry_tile_bytes(&artifact)
            .map_err(|error| serialization_error(node, CANON_GEO_GEOMETRY_TILE_VERSION, error))?;
        let mut usage = BTreeMap::new();
        usage.insert("client_source_bytes".to_string(), source.bytes.len() as u64);
        let provider_tile = artifact.provider_tile.as_ref();
        usage.insert(
            "client_features".to_string(),
            provider_tile
                .map(|tile| tile.features.len() as u64)
                .unwrap_or(0),
        );
        usage.insert(
            "client_memberships".to_string(),
            provider_tile
                .and_then(|tile| tile.client_ingest.as_ref())
                .map(|ingest| ingest.memberships.len() as u64)
                .unwrap_or(0),
        );
        Ok(GeoLeafExecution {
            output_id: "client_tile",
            output_contract: CANON_GEO_GEOMETRY_TILE_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_materialize_evidence(
        &self,
        node: &ProjectPlanNode,
    ) -> ProjectRunResult<GeoLeafExecution> {
        let section = self.required_immediate_dependency_artifact(
            node,
            "section",
            CANON_GEO_TILE_WORK_UNIT_VERSION,
        )?;
        let section_artifact: GeoTileWorkUnitArtifact =
            parse_json(node, &section.bytes, CANON_GEO_TILE_WORK_UNIT_VERSION)?;
        validate_section_candidate_reach_allows_downstream(node, &section_artifact)?;
        let rows: GeoWarehouseRowsRequest = self.required_binding_json(
            node,
            GEO_ROWS_BINDING_ID,
            &[CANON_GEO_WAREHOUSE_ROWS_VERSION],
        )?;
        self.validate_warehouse_rows_against_section(node, &section_artifact, &rows)?;
        let request = materialize_warehouse_rows(&rows)
            .map_err(|error| leaf_error(node, "materialize-evidence", error))?;
        let bytes = canonical_materialized_evidence_request_bytes(&request).map_err(|error| {
            serialization_error(node, CANON_GEO_EVIDENCE_REQUEST_VERSION, error)
        })?;
        let mut usage = BTreeMap::new();
        usage.insert(
            "warehouse_parcel_rows".to_string(),
            rows.parcel_rows.len() as u64,
        );
        usage.insert(
            "warehouse_building_parcel_rows".to_string(),
            rows.building_parcel_rows.len() as u64,
        );
        usage.insert(
            "warehouse_evidence_rows".to_string(),
            rows.evidence_rows.len() as u64,
        );
        usage.insert(
            "materialized_observations".to_string(),
            request.observations.len() as u64,
        );
        Ok(GeoLeafExecution {
            output_id: "materialize_evidence",
            output_contract: CANON_GEO_EVIDENCE_REQUEST_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_compile_evidence(
        &self,
        node: &ProjectPlanNode,
    ) -> ProjectRunResult<GeoLeafExecution> {
        let dependency = self.required_immediate_dependency_artifact(
            node,
            "materialize_evidence",
            CANON_GEO_EVIDENCE_REQUEST_VERSION,
        )?;
        let request: GeoEvidenceCompilationRequest =
            parse_json(node, &dependency.bytes, CANON_GEO_EVIDENCE_REQUEST_VERSION)?;
        let artifact = compile_evidence(&request)
            .map_err(|error| leaf_error(node, "compile-evidence", error))?;
        let bytes = canonical_evidence_compilation_bytes(&artifact).map_err(|error| {
            serialization_error(node, CANON_GEO_EVIDENCE_COMPILATION_VERSION, error)
        })?;
        let mut usage = BTreeMap::new();
        usage.insert(
            "evidence_observations".to_string(),
            request.observations.len() as u64,
        );
        usage.insert(
            "evidence_contracts".to_string(),
            request.contracts.len() as u64,
        );
        usage.insert(
            "evidence_admissions".to_string(),
            artifact.admissions.len() as u64,
        );
        usage.insert(
            "solver_hard_constraints".to_string(),
            artifact.composition_request.hard_constraints.len() as u64,
        );
        Ok(GeoLeafExecution {
            output_id: "compile_evidence",
            output_contract: CANON_GEO_EVIDENCE_COMPILATION_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_assessment_roll_owner(
        &self,
        node: &ProjectPlanNode,
    ) -> ProjectRunResult<GeoLeafExecution> {
        let request: GeoAssessmentRollOwnerRequest = self.required_binding_json(
            node,
            GEO_REQUEST_BINDING_ID,
            &[CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION],
        )?;
        let artifact = produce_assessment_roll_owner_evidence(&request)
            .map_err(|error| leaf_error(node, "assessment-roll-owner", error))?;
        let bytes = canonical_assessment_roll_owner_bytes(&artifact)
            .map_err(|error| leaf_error(node, "assessment-roll-owner serialization", error))?;
        let mut usage = BTreeMap::new();
        usage.insert(
            "assessment_roll_owner_cases".to_string(),
            artifact.summary.cases,
        );
        usage.insert(
            "assessment_roll_owner_roll_rows".to_string(),
            artifact.summary.roll_rows,
        );
        usage.insert(
            "assessment_roll_owner_party_rows".to_string(),
            artifact.summary.party_rows,
        );
        usage.insert(
            "assessment_roll_owner_added_roll_lots".to_string(),
            artifact.summary.added_roll_lots,
        );
        usage.insert(
            "assessment_roll_owner_exact_hard_observations".to_string(),
            artifact.summary.exact_hard_observations,
        );
        usage.insert(
            "assessment_roll_owner_affiliate_soft_observations".to_string(),
            artifact.summary.affiliate_soft_observations,
        );
        Ok(GeoLeafExecution {
            output_id: GEO_ASSESSMENT_ROLL_OWNER_OUTPUT_ID,
            output_contract: CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_condo_bridge(&self, node: &ProjectPlanNode) -> ProjectRunResult<GeoLeafExecution> {
        let request: GeoCondoBridgeRequest = self.required_binding_json(
            node,
            GEO_REQUEST_BINDING_ID,
            &[CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION],
        )?;
        let artifact = build_condo_bridge(&request)
            .map_err(|error| leaf_error(node, "condo-bridge", error))?;
        let bytes = canonical_condo_bridge_bytes(&artifact)
            .map_err(|error| leaf_error(node, "condo-bridge serialization", error))?;
        let mut usage = BTreeMap::new();
        usage.insert("condo_bridge_cases".to_string(), artifact.stats.cases);
        usage.insert(
            "condo_bridge_fully_reached".to_string(),
            artifact.stats.fully_reached,
        );
        usage.insert("condo_bridge_partial".to_string(), artifact.stats.partial);
        usage.insert(
            "condo_bridge_unreached".to_string(),
            artifact.stats.unreached,
        );
        usage.insert(
            "condo_bridge_truth_unit_lots".to_string(),
            artifact.stats.truth_unit_lots,
        );
        usage.insert(
            "condo_bridge_truth_unit_lots_unmapped".to_string(),
            artifact.stats.truth_unit_lots_unmapped,
        );
        Ok(GeoLeafExecution {
            output_id: GEO_CONDO_BRIDGE_OUTPUT_ID,
            output_contract: CANON_GEO_CONDO_BRIDGE_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_footprint_roll_evidence(
        &self,
        node: &ProjectPlanNode,
    ) -> ProjectRunResult<GeoLeafExecution> {
        let request: GeoFootprintRollEvidenceRequest = self.required_binding_json(
            node,
            GEO_REQUEST_BINDING_ID,
            &[CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION],
        )?;
        let evidence = materialize_footprint_roll_evidence(&request)
            .map_err(|error| leaf_error(node, "footprint-roll-evidence", error))?;
        let bytes = canonical_materialized_evidence_request_bytes(&evidence).map_err(|error| {
            serialization_error(node, CANON_GEO_EVIDENCE_REQUEST_VERSION, error)
        })?;
        let mut usage = BTreeMap::new();
        usage.insert(
            "footprint_roll_assessment_rows".to_string(),
            request.assessment_roll_rows.len() as u64,
        );
        usage.insert(
            "footprint_roll_footprint_rows".to_string(),
            request.footprint_rows.len() as u64,
        );
        usage.insert(
            "footprint_roll_contracts".to_string(),
            evidence.contracts.len() as u64,
        );
        usage.insert(
            "footprint_roll_observations".to_string(),
            evidence.observations.len() as u64,
        );
        Ok(GeoLeafExecution {
            output_id: GEO_FOOTPRINT_ROLL_EVIDENCE_OUTPUT_ID,
            output_contract: CANON_GEO_EVIDENCE_REQUEST_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_propagate(&self, node: &ProjectPlanNode) -> ProjectRunResult<GeoLeafExecution> {
        let dependency = self.required_immediate_dependency_artifact(
            node,
            "compile_evidence",
            CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        )?;
        let compilation: GeoEvidenceCompilationArtifact = parse_json(
            node,
            &dependency.bytes,
            CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        )?;
        validate_evidence_compilation_artifact(&compilation)
            .map_err(|error| leaf_error(node, "propagate.evidence_compilation", error))?;
        let budget = propagation_budget_from_node(node)?;
        let artifact = propagate(
            &compilation.composition_request,
            Some(&compilation),
            &budget,
        )
        .map_err(|error| leaf_error(node, "propagate", error))?;
        let bytes = canonical_propagation_bytes(&artifact)
            .map_err(|error| leaf_error(node, "propagate serialization", error))?;
        let mut usage = BTreeMap::new();
        usage.insert("propagation_rounds".to_string(), artifact.rounds);
        usage.insert(
            "propagation_prunings".to_string(),
            artifact.prunings.len() as u64,
        );
        usage.insert(
            "propagation_fixpoint_reached".to_string(),
            if artifact.fixpoint_reached { 1 } else { 0 },
        );
        if let Some(fallback) = &artifact.budget_fallback {
            usage.insert(format!("propagation_fallback.{}", fallback.counter), 1);
        }
        Ok(GeoLeafExecution {
            output_id: GEO_PROPAGATE_OUTPUT_ID,
            output_contract: CANON_GEO_PROPAGATION_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_explain(&self, node: &ProjectPlanNode) -> ProjectRunResult<GeoLeafExecution> {
        let input = self.required_unique_declared_dependency_artifact(
            node,
            "compile_evidence",
            CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        )?;
        let compilation: GeoEvidenceCompilationArtifact =
            parse_json(node, &input.bytes, CANON_GEO_EVIDENCE_COMPILATION_VERSION)?;
        validate_evidence_compilation_artifact(&compilation)
            .map_err(|error| leaf_error(node, "explain.evidence_compilation", error))?;
        let solved = self.required_unique_declared_dependency_artifact(
            node,
            "solve",
            CANON_GEO_COMPOSITION_VERSION,
        )?;
        let composition: GeoCompositionArtifact =
            parse_json(node, &solved.bytes, CANON_GEO_COMPOSITION_VERSION)?;
        if composition.status != GeoCompositionStatus::Conflict {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "Geo explain requires a conflict solve artifact, got {:?}",
                    composition.status
                ),
            ));
        }
        let expected_evidence =
            canonical_evidence_compilation_bytes(&compilation).map_err(|error| {
                serialization_error(node, CANON_GEO_EVIDENCE_COMPILATION_VERSION, error)
            })?;
        let expected_evidence_hash = blake3::hash(&expected_evidence).to_hex().to_string();
        let actual_evidence_hash = composition
            .evidence_compilation
            .as_ref()
            .map(|reference| reference.blake3.as_str())
            .ok_or_else(|| {
                error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    "Geo explain requires a solve artifact chained to an evidence compilation",
                )
            })?;
        if actual_evidence_hash != expected_evidence_hash {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "Geo explain evidence digest mismatch: expected {expected_evidence_hash}, got {actual_evidence_hash}"
                ),
            ));
        }
        let budget = explanation_budget_from_node(node)?;
        let order = reliability_order_from_evidence(&compilation);
        let mut artifact = minimal_core(
            &compilation.composition_request,
            &compilation,
            &order,
            &budget,
        )
        .map_err(|error| leaf_error(node, "explain.minimal_core", error))?;
        correction_sets(
            &mut artifact,
            &compilation.composition_request,
            &compilation,
            &budget,
        )
        .map_err(|error| leaf_error(node, "explain.correction_sets", error))?;
        let bytes = canonical_explanation_bytes(&artifact)
            .map_err(|error| leaf_error(node, "explain serialization", error))?;
        let mut usage = BTreeMap::new();
        usage.insert("explanation_cores".to_string(), artifact.cores.len() as u64);
        usage.insert(
            "explanation_correction_sets".to_string(),
            artifact.correction_sets.len() as u64,
        );
        usage.insert(
            "explanation_complete".to_string(),
            if artifact.explanation_complete { 1 } else { 0 },
        );
        for (key, value) in artifact.counters {
            usage.insert(format!("explanation.{key}"), value);
        }
        Ok(GeoLeafExecution {
            output_id: GEO_EXPLAIN_OUTPUT_ID,
            output_contract: CANON_GEO_EXPLANATION_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn execute_solve(&self, node: &ProjectPlanNode) -> ProjectRunResult<GeoLeafExecution> {
        let scope = self.required_exact_solve_scope(node)?;
        let section = self.required_scoped_dependency_artifact(node, &scope.bounded_section)?;
        let section_artifact: GeoTileWorkUnitArtifact =
            parse_json(node, &section.bytes, CANON_GEO_TILE_WORK_UNIT_VERSION)?;
        validate_section_candidate_reach_allows_downstream(node, &section_artifact)?;
        let input = self.required_scoped_dependency_artifact(node, &scope.evidence_compilation)?;
        let compilation: GeoEvidenceCompilationArtifact =
            parse_json(node, &input.bytes, CANON_GEO_EVIDENCE_COMPILATION_VERSION)?;
        validate_evidence_compilation_artifact(&compilation)
            .map_err(|error| leaf_error(node, "solve.evidence_compilation", error))?;
        validate_compilation_universe_against_section(node, &section_artifact, &compilation)?;
        let canonical = canonical_evidence_compilation_bytes(&compilation).map_err(|error| {
            serialization_error(node, CANON_GEO_EVIDENCE_COMPILATION_VERSION, error)
        })?;
        let evidence_reference = Some(GeoEvidenceCompilationReference {
            version: compilation.version.clone(),
            request_version: compilation.request_version.clone(),
            blake3: blake3::hash(&canonical).to_hex().to_string(),
        });
        let request = if let Some(propagation) = self.optional_declared_dependency_artifact(
            node,
            GEO_PROPAGATE_OUTPUT_ID,
            CANON_GEO_PROPAGATION_VERSION,
        )? {
            let propagation_artifact: GeoPropagationArtifact =
                parse_json(node, &propagation.bytes, CANON_GEO_PROPAGATION_VERSION)?;
            validate_propagation_artifact(&propagation_artifact)
                .map_err(|error| leaf_error(node, "solve.propagation", error))?;
            apply_prunings(&compilation.composition_request, &propagation_artifact)
                .map_err(|error| leaf_error(node, "solve.propagation", error))?
        } else {
            compilation.composition_request
        };
        let mut artifact =
            solve_composition(&request).map_err(|error| leaf_error(node, "solve", error))?;
        artifact.evidence_compilation = evidence_reference;
        let bytes = canonical_composition_bytes(&artifact)
            .map_err(|error| serialization_error(node, CANON_GEO_COMPOSITION_VERSION, error))?;
        let mut usage = BTreeMap::new();
        usage.insert(
            "bounded_section_features".to_string(),
            section_artifact.features.len() as u64,
        );
        usage.insert(
            "bounded_section_work_cells".to_string(),
            section_artifact.work_cells.len() as u64,
        );
        usage.insert(
            "composition_components".to_string(),
            artifact.summary.component_count as u64,
        );
        usage.insert(
            "composition_residual_models".to_string(),
            artifact.summary.residual_model_count,
        );
        usage.insert(
            "composition_hard_constraint_evaluations".to_string(),
            artifact.summary.hard_constraint_evaluations,
        );
        Ok(GeoLeafExecution {
            output_id: "solve",
            output_contract: CANON_GEO_COMPOSITION_VERSION,
            output_bytes: bytes,
            deterministic_usage: usage,
        })
    }

    fn required_binding_json<T: DeserializeOwned>(
        &self,
        node: &ProjectPlanNode,
        binding_id: &str,
        accepted_contracts: &[&str],
    ) -> ProjectRunResult<T> {
        let binding = self.required_binding(node, binding_id, accepted_contracts)?;
        parse_json(node, &binding.bytes, &binding.contract)
    }

    fn required_binding(
        &self,
        node: &ProjectPlanNode,
        binding_id: &str,
        accepted_contracts: &[&str],
    ) -> ProjectRunResult<&GeoExecutorInputBinding> {
        let binding = self
            .input_bindings
            .get(&(node.node_id.clone(), binding_id.to_string()))
            .ok_or_else(|| {
                error(
                    node,
                    ProjectRunErrorCode::ExecutionFailed,
                    format!(
                        "Geo executor requires binding {binding_id} for node {}",
                        node.node_id
                    ),
                )
            })?;
        verify_digest(
            &binding.content_hash,
            &binding.bytes,
            node,
            format!("input binding {binding_id}"),
        )?;
        if !accepted_contracts.contains(&binding.contract.as_str()) {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "Geo executor binding {binding_id} contract mismatch: expected one of [{}], got {}",
                    accepted_contracts.join(", "),
                    binding.contract
                ),
            ));
        }
        Ok(binding)
    }

    fn validate_no_forbidden_direct_bindings(
        &self,
        node: &ProjectPlanNode,
        command: GeoExecutorCommand,
    ) -> ProjectRunResult<()> {
        if !matches!(
            command,
            GeoExecutorCommand::CompileEvidence | GeoExecutorCommand::Solve
        ) {
            return Ok(());
        }

        let binding_ids = self
            .input_bindings
            .keys()
            .filter(|(node_id, _)| node_id == &node.node_id)
            .map(|(_, binding_id)| binding_id.as_str())
            .collect::<Vec<_>>();
        if binding_ids.is_empty() {
            return Ok(());
        }

        Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "Geo {} consumes declared dependency outputs only; direct input bindings are forbidden: {}",
                command.name(),
                binding_ids.join(",")
            ),
        ))
    }

    fn validate_tile_request_against_home_cells(
        &self,
        node: &ProjectPlanNode,
        request: &GeoTileWorkRequest,
    ) -> ProjectRunResult<()> {
        let home = self.required_immediate_dependency_artifact(
            node,
            "home_cells",
            CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
        )?;
        let artifact: GeoHomeCellAssignmentArtifact =
            parse_json(node, &home.bytes, CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION)?;
        let available = artifact
            .tile_work_features
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for feature in &request.features {
            if !available.contains(feature) {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "tile-work request feature {}:{}@{} was not produced by the home-cell dependency",
                        feature.source.source_instance_id, feature.feature_id, feature.home_cell
                    ),
                ));
            }
        }
        Ok(())
    }

    fn validate_warehouse_rows_against_section(
        &self,
        node: &ProjectPlanNode,
        section: &GeoTileWorkUnitArtifact,
        rows: &GeoWarehouseRowsRequest,
    ) -> ProjectRunResult<()> {
        let selected_level = selected_control_level(node, rows.profile.selection_level)?;
        validate_section_source_level(node, section, selected_level)?;
        let section_feature_ids = section
            .features
            .iter()
            .map(|feature| feature.feature_id.as_str())
            .collect::<BTreeSet<_>>();
        if section_feature_ids.len() != section.features.len() {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                "bounded tile section feature_id values must be unique across sources before selected-grain materialization",
            ));
        }
        let selected_grain_ids = match rows.profile.selection_level {
            GeoEntityLevel::Parcel => rows
                .parcel_rows
                .iter()
                .map(|row| row.parcel_id.as_str())
                .collect::<BTreeSet<_>>(),
            GeoEntityLevel::Building => rows
                .building_parcel_rows
                .iter()
                .map(|row| row.building_id.as_str())
                .collect::<BTreeSet<_>>(),
            other => {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "Geo executor cannot bind selected-grain rows for unsupported profile level {other:?}"
                    ),
                ));
            }
        };
        if selected_grain_ids != section_feature_ids {
            let missing = section_feature_ids
                .difference(&selected_grain_ids)
                .copied()
                .collect::<Vec<_>>()
                .join(",");
            let outside = selected_grain_ids
                .difference(&section_feature_ids)
                .copied()
                .collect::<Vec<_>>()
                .join(",");
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "warehouse selected-grain candidate universe must equal the bounded tile section feature_ids; missing_from_rows={missing}; outside_section={outside}"
                ),
            ));
        }
        validate_warehouse_auxiliary_variables(node, rows, &section_feature_ids)?;
        Ok(())
    }

    fn required_immediate_dependency_artifact(
        &self,
        node: &ProjectPlanNode,
        output_id: &str,
        contract: &str,
    ) -> ProjectRunResult<&VerifiedGeoArtifact> {
        let [producer] = node.dependencies.as_slice() else {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "Geo executor expected exactly one dependency output {output_id} for node {}",
                    node.node_id
                ),
            ));
        };
        self.required_dependency_artifact(node, producer, output_id, contract)
    }

    fn required_dependency_artifact(
        &self,
        node: &ProjectPlanNode,
        producer_node_id: &str,
        output_id: &str,
        contract: &str,
    ) -> ProjectRunResult<&VerifiedGeoArtifact> {
        let artifact = self
            .dependency_outputs
            .get(&(producer_node_id.to_string(), output_id.to_string()))
            .ok_or_else(|| {
                error(
                    node,
                    ProjectRunErrorCode::ExecutionFailed,
                    format!(
                        "Geo executor requires dependency output {producer_node_id}:{output_id} before node {}",
                        node.node_id
                    ),
                )
            })?;
        if artifact.contract != contract {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "dependency output {producer_node_id}:{output_id} contract mismatch: expected {contract}, got {}",
                    artifact.contract
                ),
            ));
        }
        verify_digest(
            &artifact.content_hash,
            &artifact.bytes,
            node,
            format!("dependency output {producer_node_id}:{output_id}"),
        )?;
        ensure_canonical_artifact_bytes(node, contract, &artifact.bytes)?;
        Ok(artifact)
    }

    fn required_exact_solve_scope(
        &self,
        node: &ProjectPlanNode,
    ) -> ProjectRunResult<&GeoPlanExactSolveScope> {
        let scope = self.exact_solve_scopes.get(&node.node_id).ok_or_else(|| {
            error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                "Geo solve requires the plan's explicit exact_solve_scope; implicit section selection is forbidden",
            )
        })?;
        if scope.bounded_section.output_contract != CANON_GEO_TILE_WORK_UNIT_VERSION
            || scope.evidence_compilation.output_contract != CANON_GEO_EVIDENCE_COMPILATION_VERSION
            || scope.component_scope
                != GeoPlanComponentScope::ActualConnectedComponentsOfCompiledConstraintIncidence
            || scope.component_key_field != "canon_geo_composition.v0.factorization[].key"
        {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                "Geo executor exact_solve_scope does not bind the required bounded section, compiled evidence, and incidence-component contract",
            ));
        }
        Ok(scope)
    }

    fn required_scoped_dependency_artifact(
        &self,
        node: &ProjectPlanNode,
        artifact: &GeoPlanProducedArtifactRef,
    ) -> ProjectRunResult<&VerifiedGeoArtifact> {
        if !node
            .dependencies
            .iter()
            .any(|dependency| dependency == &artifact.producer_node_id)
        {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "exact_solve_scope artifact {}:{} must be declared as a direct dependency of the solve node",
                    artifact.producer_node_id, artifact.output_id
                ),
            ));
        }
        self.required_dependency_artifact(
            node,
            &artifact.producer_node_id,
            &artifact.output_id,
            &artifact.output_contract,
        )
    }

    fn optional_declared_dependency_artifact(
        &self,
        node: &ProjectPlanNode,
        output_id: &str,
        contract: &str,
    ) -> ProjectRunResult<Option<&VerifiedGeoArtifact>> {
        let matching = node
            .dependencies
            .iter()
            .filter_map(|producer| {
                self.dependency_outputs
                    .get(&(producer.clone(), output_id.to_string()))
                    .map(|artifact| (producer, artifact))
            })
            .collect::<Vec<_>>();
        match matching.as_slice() {
            [] => Ok(None),
            [(_, artifact)] => {
                if artifact.contract != contract {
                    return Err(error(
                        node,
                        ProjectRunErrorCode::ArtifactContract,
                        format!(
                            "dependency output {}:{output_id} contract mismatch: expected {contract}, got {}",
                            artifact.producer_node_id, artifact.contract
                        ),
                    ));
                }
                verify_digest(
                    &artifact.content_hash,
                    &artifact.bytes,
                    node,
                    format!(
                        "dependency output {}:{output_id}",
                        artifact.producer_node_id
                    ),
                )?;
                ensure_canonical_artifact_bytes(node, contract, &artifact.bytes)?;
                Ok(Some(*artifact))
            }
            _ => Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!("Geo executor expected at most one dependency output {output_id}"),
            )),
        }
    }

    fn required_unique_declared_dependency_artifact(
        &self,
        node: &ProjectPlanNode,
        output_id: &str,
        contract: &str,
    ) -> ProjectRunResult<&VerifiedGeoArtifact> {
        let matching = node
            .dependencies
            .iter()
            .filter_map(|producer| {
                self.dependency_outputs
                    .get(&(producer.clone(), output_id.to_string()))
            })
            .collect::<Vec<_>>();
        let [artifact] = matching.as_slice() else {
            return Err(error(
                node,
                ProjectRunErrorCode::ExecutionFailed,
                format!("Geo executor requires exactly one direct dependency output {output_id}"),
            ));
        };
        if artifact.contract != contract {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "dependency output {}:{output_id} contract mismatch: expected {contract}, got {}",
                    artifact.producer_node_id, artifact.contract
                ),
            ));
        }
        verify_digest(
            &artifact.content_hash,
            &artifact.bytes,
            node,
            format!(
                "dependency output {}:{output_id}",
                artifact.producer_node_id
            ),
        )?;
        ensure_canonical_artifact_bytes(node, contract, &artifact.bytes)?;
        Ok(artifact)
    }

    fn remember_output(
        &mut self,
        node: &ProjectPlanNode,
        output_id: &str,
        contract: &str,
        bytes: Vec<u8>,
    ) {
        let artifact = VerifiedGeoArtifact {
            producer_node_id: node.node_id.clone(),
            output_id: output_id.to_string(),
            contract: contract.to_string(),
            content_hash: geo_executor_content_hash(&bytes),
            bytes,
        };
        self.dependency_outputs.insert(
            (
                artifact.producer_node_id.clone(),
                artifact.output_id.clone(),
            ),
            artifact,
        );
    }
}

fn validate_compilation_universe_against_section(
    node: &ProjectPlanNode,
    section: &GeoTileWorkUnitArtifact,
    compilation: &GeoEvidenceCompilationArtifact,
) -> ProjectRunResult<()> {
    let selected_level = selected_control_level(
        node,
        compilation.composition_request.profile.selection_level,
    )?;
    validate_section_source_level(node, section, selected_level)?;
    let section_ids = section
        .features
        .iter()
        .map(|feature| feature.feature_id.as_str())
        .collect::<BTreeSet<_>>();
    if section_ids.len() != section.features.len() {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            "bounded tile section feature_id values must be unique before exact solving",
        ));
    }
    let selected_ids = match compilation.composition_request.profile.selection_level {
        GeoEntityLevel::Parcel => compilation
            .composition_request
            .universe
            .parcels
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        GeoEntityLevel::Building => compilation
            .composition_request
            .universe
            .buildings
            .iter()
            .map(|building| building.id.as_str())
            .collect::<BTreeSet<_>>(),
        other => {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!("Geo executor cannot exact-solve unsupported profile level {other:?}"),
            ));
        }
    };
    if selected_ids != section_ids {
        let missing = section_ids
            .difference(&selected_ids)
            .copied()
            .collect::<Vec<_>>()
            .join(",");
        let outside = selected_ids
            .difference(&section_ids)
            .copied()
            .collect::<Vec<_>>()
            .join(",");
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "compiled evidence candidate universe must equal the exact_solve_scope bounded section; missing_from_compilation={missing}; outside_section={outside}"
            ),
        ));
    }
    validate_compilation_auxiliary_variables(node, compilation, &selected_ids)?;
    Ok(())
}

fn validate_section_candidate_reach_allows_downstream(
    node: &ProjectPlanNode,
    section: &GeoTileWorkUnitArtifact,
) -> ProjectRunResult<()> {
    if section.candidate_reach.status != GeoTileCandidateReachStatus::FailedAgainstReference {
        return Ok(());
    }
    let reference_id = section
        .candidate_reach
        .reference
        .as_ref()
        .map(|reference| reference.reference_id.as_str())
        .unwrap_or("<unknown>");
    let missing = section.candidate_reach.missing_reference_count;
    Err(error(
        node,
        ProjectRunErrorCode::ArtifactContract,
        format!(
            "bounded tile section candidate reach failed against reference {reference_id}; missing_reference_count={missing}; downstream solving cannot repair an unreachable candidate"
        ),
    ))
}

fn selected_control_level(
    node: &ProjectPlanNode,
    selection_level: GeoEntityLevel,
) -> ProjectRunResult<GeoControlEntityLevel> {
    match selection_level {
        GeoEntityLevel::Parcel => Ok(GeoControlEntityLevel::Parcel),
        GeoEntityLevel::Building => Ok(GeoControlEntityLevel::Building),
        other => Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "Geo executor cannot bind selected-grain rows for unsupported profile level {other:?}"
            ),
        )),
    }
}

fn validate_section_source_level(
    node: &ProjectPlanNode,
    section: &GeoTileWorkUnitArtifact,
    selected_level: GeoControlEntityLevel,
) -> ProjectRunResult<()> {
    for feature in &section.features {
        match feature.source.native_entity_level() {
            Some(actual) if actual == selected_level => {}
            Some(actual) => {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "bounded tile section feature source native entity level must equal the profile selection_level before comparing feature_id values; source_instance_id={}; feature_id={}; expected={selected_level:?}; actual={actual:?}",
                        feature.source.source_instance_id, feature.feature_id
                    ),
                ));
            }
            None => {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "bounded tile section feature source must declare a native entity level for selected-grain equality; source_instance_id={}; feature_id={}",
                        feature.source.source_instance_id, feature.feature_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_warehouse_auxiliary_variables(
    node: &ProjectPlanNode,
    rows: &GeoWarehouseRowsRequest,
    selected_grain_ids: &BTreeSet<&str>,
) -> ProjectRunResult<()> {
    match rows.profile.selection_level {
        GeoEntityLevel::Parcel => {
            for row in &rows.building_parcel_rows {
                match row.parcel_id.as_deref() {
                    Some(parcel_id) if selected_grain_ids.contains(parcel_id) => {}
                    Some(parcel_id) => {
                        return Err(error(
                            node,
                            ProjectRunErrorCode::ArtifactContract,
                            format!(
                                "parcel-profile auxiliary building incidence must point into the selected bounded section parcels; building_id={}; parcel_id={parcel_id}",
                                row.building_id
                            ),
                        ));
                    }
                    None => {
                        return Err(error(
                            node,
                            ProjectRunErrorCode::ArtifactContract,
                            format!(
                                "parcel-profile auxiliary building must have source-member incidence into selected section parcels; building_id={}",
                                row.building_id
                            ),
                        ));
                    }
                }
            }
        }
        GeoEntityLevel::Building => {
            if let Some(row) = rows.parcel_rows.first() {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "building-profile warehouse rows cannot declare unbound auxiliary parcel candidates; parcel_id={}",
                        row.parcel_id
                    ),
                ));
            }
            if let Some(row) = rows
                .building_parcel_rows
                .iter()
                .find(|row| row.parcel_id.is_some())
            {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "building-profile warehouse rows cannot bind auxiliary parcel incidences outside selected-building structure; building_id={}; parcel_id={}",
                        row.building_id,
                        row.parcel_id.as_deref().unwrap_or("null")
                    ),
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_compilation_auxiliary_variables(
    node: &ProjectPlanNode,
    compilation: &GeoEvidenceCompilationArtifact,
    selected_ids: &BTreeSet<&str>,
) -> ProjectRunResult<()> {
    match compilation.composition_request.profile.selection_level {
        GeoEntityLevel::Parcel => {
            for building in &compilation.composition_request.universe.buildings {
                if building.parcel_ids.is_empty() {
                    return Err(error(
                        node,
                        ProjectRunErrorCode::ArtifactContract,
                        format!(
                            "compiled parcel-profile auxiliary building must have source-member incidence into selected section parcels; building_id={}",
                            building.id
                        ),
                    ));
                }
                for parcel_id in &building.parcel_ids {
                    if !selected_ids.contains(parcel_id.as_str()) {
                        return Err(error(
                            node,
                            ProjectRunErrorCode::ArtifactContract,
                            format!(
                                "compiled parcel-profile auxiliary building incidence must point into the selected bounded section parcels; building_id={}; parcel_id={parcel_id}",
                                building.id
                            ),
                        ));
                    }
                }
            }
        }
        GeoEntityLevel::Building => {
            if let Some(parcel_id) = compilation.composition_request.universe.parcels.first() {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "compiled building-profile universe cannot carry unbound auxiliary parcel candidates; parcel_id={parcel_id}",
                    ),
                ));
            }
            if let Some(building) = compilation
                .composition_request
                .universe
                .buildings
                .iter()
                .find(|building| !building.parcel_ids.is_empty())
            {
                return Err(error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    format!(
                        "compiled building-profile universe cannot carry auxiliary parcel incidences outside selected-building structure; building_id={}; parcel_ids={}",
                        building.id,
                        building.parcel_ids.join(",")
                    ),
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

impl Default for GeoProjectNodeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectNodeExecutor for GeoProjectNodeExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> ProjectRunResult<ProjectNodeExecutionResult> {
        self.execute_typed(node, context)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeoExecutorCommand {
    MaterializeHomeCells,
    TileWork,
    ClientTileIngest,
    MaterializeEvidence,
    CompileEvidence,
    AssessmentRollOwner,
    CondoBridge,
    FootprintRollEvidence,
    Propagate,
    Explain,
    Solve,
}

impl GeoExecutorCommand {
    const SUPPORTED: [Self; 11] = [
        Self::MaterializeHomeCells,
        Self::TileWork,
        Self::ClientTileIngest,
        Self::MaterializeEvidence,
        Self::CompileEvidence,
        Self::AssessmentRollOwner,
        Self::CondoBridge,
        Self::FootprintRollEvidence,
        Self::Propagate,
        Self::Explain,
        Self::Solve,
    ];

    fn from_command(node: &ProjectPlanNode) -> ProjectRunResult<Self> {
        match node.command.as_str() {
            GEO_MATERIALIZE_HOME_CELLS_COMMAND => Ok(Self::MaterializeHomeCells),
            GEO_TILE_WORK_COMMAND => Ok(Self::TileWork),
            GEO_CLIENT_TILE_INGEST_STAGE_COMMAND => Ok(Self::ClientTileIngest),
            GEO_MATERIALIZE_EVIDENCE_COMMAND => Ok(Self::MaterializeEvidence),
            GEO_COMPILE_EVIDENCE_COMMAND => Ok(Self::CompileEvidence),
            GEO_ASSESSMENT_ROLL_OWNER_STAGE_COMMAND => Ok(Self::AssessmentRollOwner),
            GEO_CONDO_BRIDGE_STAGE_COMMAND => Ok(Self::CondoBridge),
            GEO_FOOTPRINT_ROLL_EVIDENCE_STAGE_COMMAND => Ok(Self::FootprintRollEvidence),
            GEO_PROPAGATE_STAGE_COMMAND => Ok(Self::Propagate),
            GEO_EXPLAIN_STAGE_COMMAND => Ok(Self::Explain),
            GEO_SOLVE_COMMAND => Ok(Self::Solve),
            actual => Err(error(
                node,
                ProjectRunErrorCode::ExecutionFailed,
                format!("Geo executor does not implement command {actual}"),
            )),
        }
    }

    fn expected_kind(self) -> ProjectPlanNodeKind {
        match self {
            Self::MaterializeHomeCells => ProjectPlanNodeKind::Normalize,
            Self::TileWork => ProjectPlanNodeKind::Block,
            Self::ClientTileIngest => ProjectPlanNodeKind::Index,
            Self::MaterializeEvidence
            | Self::CompileEvidence
            | Self::AssessmentRollOwner
            | Self::CondoBridge
            | Self::FootprintRollEvidence => ProjectPlanNodeKind::Evidence,
            Self::Propagate | Self::Explain | Self::Solve => ProjectPlanNodeKind::Solve,
        }
    }

    fn expected_output_id(self) -> &'static str {
        match self {
            Self::MaterializeHomeCells => "home_cells",
            Self::TileWork => "section",
            Self::ClientTileIngest => "client_tile",
            Self::MaterializeEvidence => "materialize_evidence",
            Self::CompileEvidence => "compile_evidence",
            Self::AssessmentRollOwner => GEO_ASSESSMENT_ROLL_OWNER_OUTPUT_ID,
            Self::CondoBridge => GEO_CONDO_BRIDGE_OUTPUT_ID,
            Self::FootprintRollEvidence => GEO_FOOTPRINT_ROLL_EVIDENCE_OUTPUT_ID,
            Self::Propagate => GEO_PROPAGATE_OUTPUT_ID,
            Self::Explain => GEO_EXPLAIN_OUTPUT_ID,
            Self::Solve => "solve",
        }
    }

    fn expected_dependencies(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::MaterializeHomeCells => &[],
            Self::ClientTileIngest => &[],
            Self::TileWork => &[("home_cells", CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION)],
            Self::MaterializeEvidence => &[("section", CANON_GEO_TILE_WORK_UNIT_VERSION)],
            Self::CompileEvidence => {
                &[("materialize_evidence", CANON_GEO_EVIDENCE_REQUEST_VERSION)]
            }
            Self::AssessmentRollOwner => &[],
            Self::CondoBridge => &[],
            Self::FootprintRollEvidence => &[],
            Self::Propagate => &[("compile_evidence", CANON_GEO_EVIDENCE_COMPILATION_VERSION)],
            Self::Explain => &[
                ("compile_evidence", CANON_GEO_EVIDENCE_COMPILATION_VERSION),
                ("solve", CANON_GEO_COMPOSITION_VERSION),
            ],
            Self::Solve => &[("compile_evidence", CANON_GEO_EVIDENCE_COMPILATION_VERSION)],
        }
    }

    fn requires_exact_dependency_count(self) -> bool {
        !matches!(self, Self::Solve)
    }

    fn name(self) -> &'static str {
        match self {
            Self::MaterializeHomeCells => "materialize-home-cells",
            Self::TileWork => "tile-work",
            Self::ClientTileIngest => "client-tile-ingest",
            Self::MaterializeEvidence => "materialize-evidence",
            Self::CompileEvidence => "compile-evidence",
            Self::AssessmentRollOwner => "assessment-roll-owner",
            Self::CondoBridge => "condo-bridge",
            Self::FootprintRollEvidence => "footprint-roll-evidence",
            Self::Propagate => "propagate",
            Self::Explain => "explain",
            Self::Solve => "solve",
        }
    }
}

fn explanation_budget_from_node(node: &ProjectPlanNode) -> ProjectRunResult<GeoExplanationBudget> {
    let default = GeoExplanationBudget::default();
    Ok(GeoExplanationBudget {
        max_core_solves: node
            .limits
            .get("geo.explanation.max_core_solves")
            .copied()
            .unwrap_or(default.max_core_solves),
        max_cores: node
            .limits
            .get("geo.explanation.max_cores")
            .copied()
            .unwrap_or(default.max_cores),
        max_hitting_sets: node
            .limits
            .get("geo.explanation.max_hitting_sets")
            .copied()
            .unwrap_or(default.max_hitting_sets),
    })
}

fn propagation_budget_from_node(node: &ProjectPlanNode) -> ProjectRunResult<GeoPropagationBudget> {
    let default = GeoPropagationBudget::default();
    let max_hall_subset_size = node
        .limits
        .get("geo.propagation.max_hall_subset_size")
        .copied()
        .map(|value| {
            usize::try_from(value).map_err(|_| {
                error(
                    node,
                    ProjectRunErrorCode::ArtifactContract,
                    "geo.propagation.max_hall_subset_size exceeds usize",
                )
            })
        })
        .transpose()?
        .unwrap_or(default.max_hall_subset_size);
    Ok(GeoPropagationBudget {
        max_fixpoint_rounds: node
            .limits
            .get("geo.propagation.max_fixpoint_rounds")
            .copied()
            .unwrap_or(default.max_fixpoint_rounds),
        max_hall_subset_size,
        max_subset_sum_states: node
            .limits
            .get("geo.propagation.max_subset_sum_states")
            .copied()
            .unwrap_or(default.max_subset_sum_states),
    })
}

fn validate_node_contract(
    node: &ProjectPlanNode,
    context: &ProjectNodeExecutionContext,
    command: GeoExecutorCommand,
) -> ProjectRunResult<()> {
    if node.kind != command.expected_kind() {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "Geo executor node kind mismatch for {}: expected {:?}, got {:?}",
                node.command,
                command.expected_kind(),
                node.kind
            ),
        ));
    }
    if node.class != ProjectPlanNodeClass::Computation {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            "Geo executor can run only computation nodes",
        ));
    }
    validate_side_effects(node)?;
    if node.outputs.len() != 1
        || node.outputs[0].output_id != command.expected_output_id()
        || node.outputs[0].materialization != ProjectPlanOutputMaterialization::PlannedArtifact
    {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "Geo executor expected one planned output id {}",
                command.expected_output_id()
            ),
        ));
    }
    let declared_dependencies = node.dependencies.iter().cloned().collect::<BTreeSet<_>>();
    let context_dependencies = context
        .dependency_semantic_hashes
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if declared_dependencies != context_dependencies {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            "Geo executor dependency semantic-hash context does not match declared dependencies",
        ));
    }
    let context_output_dependencies = context
        .dependency_outputs
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if declared_dependencies != context_output_dependencies {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            "Geo executor dependency-output context does not match declared dependencies",
        ));
    }
    for (dependency, semantic_hash) in &context.dependency_semantic_hashes {
        validate_blake3_digest(semantic_hash).map_err(|message| {
            error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!("dependency {dependency} semantic hash is invalid: {message}"),
            )
        })?;
    }
    Ok(())
}

fn validate_side_effects(node: &ProjectPlanNode) -> ProjectRunResult<()> {
    let mut kinds = BTreeSet::new();
    for effect in &node.side_effects {
        if !matches!(
            effect.kind,
            ProjectPlanSideEffectKind::ReadsInput | ProjectPlanSideEffectKind::WritesArtifact
        ) {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                "Geo executor is offline and may only read declared inputs and write declared artifacts",
            ));
        }
        if !kinds.insert(effect.kind) {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                "Geo executor side-effect declarations must be distinct",
            ));
        }
    }
    let expected = BTreeSet::from([
        ProjectPlanSideEffectKind::ReadsInput,
        ProjectPlanSideEffectKind::WritesArtifact,
    ]);
    if kinds != expected {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            "Geo executor nodes must declare read-input and write-artifact side effects",
        ));
    }
    Ok(())
}

fn verify_project_dependency_output(
    node: &ProjectPlanNode,
    producer_node_id: &str,
    output: &ProjectDependencyOutput,
) -> ProjectRunResult<()> {
    verify_digest(
        &output.content_digest,
        &output.bytes,
        node,
        format!("dependency output {producer_node_id}:{}", output.output_id),
    )?;
    if output.byte_count != output.bytes.len() as u64 {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "dependency output {producer_node_id}:{} byte_count does not match bytes",
                output.output_id
            ),
        ));
    }
    Ok(())
}

fn contract_for_output_id(output_id: &str) -> Option<&'static str> {
    match output_id {
        "home_cells" => Some(CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION),
        "section" => Some(CANON_GEO_TILE_WORK_UNIT_VERSION),
        "client_tile" => Some(CANON_GEO_GEOMETRY_TILE_VERSION),
        "materialize_evidence" => Some(CANON_GEO_EVIDENCE_REQUEST_VERSION),
        "compile_evidence" => Some(CANON_GEO_EVIDENCE_COMPILATION_VERSION),
        GEO_ASSESSMENT_ROLL_OWNER_OUTPUT_ID => Some(CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION),
        GEO_CONDO_BRIDGE_OUTPUT_ID => Some(CANON_GEO_CONDO_BRIDGE_VERSION),
        GEO_FOOTPRINT_ROLL_EVIDENCE_OUTPUT_ID => Some(CANON_GEO_EVIDENCE_REQUEST_VERSION),
        GEO_PROPAGATE_OUTPUT_ID => Some(CANON_GEO_PROPAGATION_VERSION),
        GEO_EXPLAIN_OUTPUT_ID => Some(CANON_GEO_EXPLANATION_VERSION),
        "solve" => Some(CANON_GEO_COMPOSITION_VERSION),
        _ => None,
    }
}

fn validate_expected_dependency(
    node: &ProjectPlanNode,
    command: GeoExecutorCommand,
    outputs: &BTreeMap<(String, String), VerifiedGeoArtifact>,
) -> ProjectRunResult<()> {
    let expected = command.expected_dependencies();
    if expected.is_empty() {
        if node.dependencies.is_empty() {
            return Ok(());
        }
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "Geo {} must start from explicit bindings, not dependency outputs",
                command.name()
            ),
        ));
    }
    if command.requires_exact_dependency_count() && node.dependencies.len() != expected.len() {
        return Err(error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "Geo command {} requires exactly {} direct dependency outputs",
                node.command,
                expected.len()
            ),
        ));
    }
    for (output_id, contract) in expected {
        let matching = node
            .dependencies
            .iter()
            .filter_map(|producer| {
                outputs
                    .get(&(producer.to_string(), (*output_id).to_string()))
                    .map(|artifact| (producer, artifact))
            })
            .collect::<Vec<_>>();
        let [(producer, artifact)] = matching.as_slice() else {
            return Err(error(
                node,
                ProjectRunErrorCode::ExecutionFailed,
                format!(
                    "Geo command {} requires exactly one direct dependency output {output_id}",
                    node.command
                ),
            ));
        };
        if artifact.contract != *contract {
            return Err(error(
                node,
                ProjectRunErrorCode::ArtifactContract,
                format!(
                    "Geo dependency output {}:{output_id} must have contract {contract}, got {}",
                    producer, artifact.contract
                ),
            ));
        }
    }
    Ok(())
}

fn parse_json<T: DeserializeOwned>(
    node: &ProjectPlanNode,
    bytes: &[u8],
    contract: &str,
) -> ProjectRunResult<T> {
    serde_json::from_slice(bytes).map_err(|parse_error| {
        error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!("failed to parse {contract} JSON: {parse_error}"),
        )
    })
}

pub(crate) fn validate_canonical_geo_artifact_bytes(
    node_id: &str,
    contract: &str,
    bytes: &[u8],
) -> ProjectRunResult<()> {
    ensure_canonical_artifact_bytes(node_id, contract, bytes)
}

fn ensure_canonical_artifact_bytes(
    node: impl NodeErrorTarget,
    contract: &str,
    bytes: &[u8],
) -> ProjectRunResult<()> {
    match contract {
        CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION => {
            let artifact: GeoHomeCellAssignmentArtifact =
                parse_json_target(&node, bytes, CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION)?;
            if artifact.version != CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION {
                return Err(target_error(
                    &node,
                    ProjectRunErrorCode::ArtifactContract,
                    "home-cell assignment artifact declares the wrong version",
                ));
            }
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_home_cell_assignment_bytes(&artifact),
            )
        }
        CANON_GEO_TILE_WORK_UNIT_VERSION => {
            let artifact: GeoTileWorkUnitArtifact =
                parse_json_target(&node, bytes, CANON_GEO_TILE_WORK_UNIT_VERSION)?;
            if artifact.version != CANON_GEO_TILE_WORK_UNIT_VERSION {
                return Err(target_error(
                    &node,
                    ProjectRunErrorCode::ArtifactContract,
                    "tile-work artifact declares the wrong version",
                ));
            }
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_tile_work_unit_bytes(&artifact),
            )
        }
        CANON_GEO_GEOMETRY_TILE_VERSION => {
            let artifact: GeoGeometryTileArtifact =
                parse_json_target(&node, bytes, CANON_GEO_GEOMETRY_TILE_VERSION)?;
            if artifact.version != CANON_GEO_GEOMETRY_TILE_VERSION {
                return Err(target_error(
                    &node,
                    ProjectRunErrorCode::ArtifactContract,
                    "geometry tile artifact declares the wrong version",
                ));
            }
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_geometry_tile_bytes(&artifact),
            )
        }
        CANON_GEO_EVIDENCE_REQUEST_VERSION => {
            let request: GeoEvidenceCompilationRequest =
                parse_json_target(&node, bytes, CANON_GEO_EVIDENCE_REQUEST_VERSION)?;
            if request.version != CANON_GEO_EVIDENCE_REQUEST_VERSION {
                return Err(target_error(
                    &node,
                    ProjectRunErrorCode::ArtifactContract,
                    "evidence request declares the wrong version",
                ));
            }
            compile_evidence(&request)
                .map_err(|error| leaf_error_target(&node, "evidence request validation", error))?;
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_materialized_evidence_request_bytes(&request),
            )
        }
        CANON_GEO_EVIDENCE_COMPILATION_VERSION => {
            let artifact: GeoEvidenceCompilationArtifact =
                parse_json_target(&node, bytes, CANON_GEO_EVIDENCE_COMPILATION_VERSION)?;
            validate_evidence_compilation_artifact(&artifact).map_err(|error| {
                leaf_error_target(&node, "evidence compilation validation", error)
            })?;
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_evidence_compilation_bytes(&artifact),
            )
        }
        CANON_GEO_PROPAGATION_VERSION => {
            let artifact: GeoPropagationArtifact =
                parse_json_target(&node, bytes, CANON_GEO_PROPAGATION_VERSION)?;
            validate_propagation_artifact(&artifact)
                .map_err(|error| leaf_error_target(&node, "propagation validation", error))?;
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_propagation_bytes(&artifact),
            )
        }
        CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION => {
            let artifact: GeoAssessmentRollOwnerArtifact =
                parse_json_target(&node, bytes, CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION)?;
            validate_assessment_roll_owner_artifact(&artifact).map_err(|error| {
                leaf_error_target(&node, "assessment-roll-owner validation", error)
            })?;
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_assessment_roll_owner_bytes(&artifact),
            )
        }
        CANON_GEO_CONDO_BRIDGE_VERSION => {
            let artifact: GeoCondoBridgeArtifact =
                parse_json_target(&node, bytes, CANON_GEO_CONDO_BRIDGE_VERSION)?;
            validate_condo_bridge_artifact(&artifact)
                .map_err(|error| leaf_error_target(&node, "condo-bridge validation", error))?;
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_condo_bridge_bytes(&artifact),
            )
        }
        CANON_GEO_EXPLANATION_VERSION => {
            let artifact: GeoExplanationArtifact =
                parse_json_target(&node, bytes, CANON_GEO_EXPLANATION_VERSION)?;
            validate_explanation_artifact(&artifact)
                .map_err(|error| leaf_error_target(&node, "explanation validation", error))?;
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_explanation_bytes(&artifact),
            )
        }
        CANON_GEO_COMPOSITION_VERSION => {
            let artifact: GeoCompositionArtifact =
                parse_json_target(&node, bytes, CANON_GEO_COMPOSITION_VERSION)?;
            if artifact.version != CANON_GEO_COMPOSITION_VERSION
                || artifact.request_version != CANON_GEO_COMPOSITION_REQUEST_VERSION
            {
                return Err(target_error(
                    &node,
                    ProjectRunErrorCode::ArtifactContract,
                    "composition artifact declares the wrong version",
                ));
            }
            require_exact_bytes(
                &node,
                contract,
                bytes,
                canonical_composition_bytes(&artifact),
            )
        }
        actual => Err(target_error(
            &node,
            ProjectRunErrorCode::ArtifactContract,
            format!("unsupported Geo artifact contract {actual}"),
        )),
    }
}

fn require_exact_bytes(
    node: &impl NodeErrorTarget,
    contract: &str,
    actual: &[u8],
    canonical: Result<Vec<u8>, impl Error>,
) -> ProjectRunResult<()> {
    let canonical = canonical.map_err(|error| {
        target_error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!("failed to canonicalize {contract}: {error}"),
        )
    })?;
    if actual != canonical.as_slice() {
        return Err(target_error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!("{contract} bytes are not canonical executor output"),
        ));
    }
    Ok(())
}

fn parse_json_target<T: DeserializeOwned>(
    node: &impl NodeErrorTarget,
    bytes: &[u8],
    contract: &str,
) -> ProjectRunResult<T> {
    serde_json::from_slice(bytes).map_err(|error| {
        target_error(
            node,
            ProjectRunErrorCode::ArtifactContract,
            format!("failed to parse {contract} JSON: {error}"),
        )
    })
}

fn verify_digest(
    expected: &str,
    bytes: &[u8],
    node: impl NodeErrorTarget,
    label: impl AsRef<str>,
) -> ProjectRunResult<()> {
    validate_blake3_digest(expected).map_err(|message| {
        target_error(
            &node,
            ProjectRunErrorCode::ArtifactContract,
            format!("{} digest is invalid: {message}", label.as_ref()),
        )
    })?;
    let actual = geo_executor_content_hash(bytes);
    if actual != expected {
        return Err(target_error(
            &node,
            ProjectRunErrorCode::ArtifactContract,
            format!(
                "{} digest mismatch: expected {expected}, got {actual}",
                label.as_ref()
            ),
        ));
    }
    Ok(())
}

fn validate_blake3_digest(value: &str) -> Result<(), String> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err("expected blake3:<64 lowercase hex>".to_string());
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected blake3:<64 lowercase hex>".to_string());
    }
    Ok(())
}

pub fn geo_executor_content_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn leaf_error(node: &ProjectPlanNode, label: &str, error: impl Error) -> ProjectRunError {
    leaf_error_target(node, label, error)
}

fn leaf_error_target(
    node: &impl NodeErrorTarget,
    label: &str,
    error: impl Error,
) -> ProjectRunError {
    target_error(
        node,
        ProjectRunErrorCode::ArtifactContract,
        format!("Geo {label} contract failed: {error}"),
    )
}

fn serialization_error(
    node: &ProjectPlanNode,
    contract: &str,
    error: serde_json::Error,
) -> ProjectRunError {
    serialization_error_target(node, contract, error)
}

fn serialization_error_target(
    node: &impl NodeErrorTarget,
    contract: &str,
    error: serde_json::Error,
) -> ProjectRunError {
    target_error(
        node,
        ProjectRunErrorCode::ArtifactContract,
        format!("failed to serialize {contract}: {error}"),
    )
}

fn error(
    node: &ProjectPlanNode,
    code: ProjectRunErrorCode,
    message: impl Into<String>,
) -> ProjectRunError {
    ProjectRunError::new(code, Some(node.node_id.clone()), message)
}

fn target_error(
    node: &impl NodeErrorTarget,
    code: ProjectRunErrorCode,
    message: impl Into<String>,
) -> ProjectRunError {
    ProjectRunError::new(code, Some(node.node_id().to_string()), message)
}

trait NodeErrorTarget {
    fn node_id(&self) -> &str;
}

impl NodeErrorTarget for ProjectPlanNode {
    fn node_id(&self) -> &str {
        &self.node_id
    }
}

impl NodeErrorTarget for &ProjectPlanNode {
    fn node_id(&self) -> &str {
        &self.node_id
    }
}

impl NodeErrorTarget for String {
    fn node_id(&self) -> &str {
        self.as_str()
    }
}

impl NodeErrorTarget for &String {
    fn node_id(&self) -> &str {
        self.as_str()
    }
}

impl NodeErrorTarget for &str {
    fn node_id(&self) -> &str {
        self
    }
}

impl fmt::Debug for GeoProjectNodeExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeoProjectNodeExecutor")
            .field("executor_id", &GEO_EXECUTOR_ID)
            .field("executor_version", &GEO_EXECUTOR_VERSION)
            .field(
                "input_bindings",
                &self.input_bindings.keys().collect::<Vec<_>>(),
            )
            .field(
                "dependency_outputs",
                &self.dependency_outputs.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}
