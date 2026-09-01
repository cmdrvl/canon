#![forbid(unsafe_code)]

//! Resumable Canon Geo run projection over `canon.project.run.v2`.
//!
//! This module does not schedule work itself and does not dispatch through a
//! shell.  It validates a `canon_geo_plan.v0`, binds typed local artifacts, and
//! delegates execution/reuse/publication to the shared project runner through an
//! injected in-process executor.

use crate::{
    fs_safety::{PlannedAccess, resolve_workspace_path},
    geo::{
        CANON_GEO_COMPOSITION_VERSION, CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
        CANON_GEO_HOME_CELL_ROWS_VERSION, CANON_GEO_PLAN_VERSION,
        CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION,
        CANON_GEO_WAREHOUSE_ROWS_VERSION, GeoCompositionArtifact, GeoCompositionStatus, GeoPlan,
        GeoPlanError, GeoPlanExternalRequest, GeoPlanGrainStatus, GeoPlanNodeOverlay, GeoPlanStage,
        GeoPlanStatus, GeoTileWorkUnitArtifact, executor::GEO_COMPILE_EVIDENCE_COMMAND,
        executor::GEO_MATERIALIZE_EVIDENCE_COMMAND, executor::GEO_MATERIALIZE_HOME_CELLS_COMMAND,
        executor::GEO_REQUEST_BINDING_ID, executor::GEO_ROWS_BINDING_ID,
        executor::GEO_SOLVE_COMMAND, executor::GEO_TILE_WORK_COMMAND,
        executor::GeoExecutorDependencyOutput, executor::GeoExecutorInputBinding,
        executor::GeoProjectNodeExecutor, executor::validate_canonical_geo_artifact_bytes,
        validate_geo_plan,
    },
    project::{
        CANON_PROJECT_RUN_VERSION, ProjectExtensionDagNode, ProjectExtensionDagOutput,
        ProjectExtensionDagRequest, ProjectNodeExecutor, ProjectPlan, ProjectPlanCacheDecision,
        ProjectPlanHashRef, ProjectPlanNode, ProjectPlanOutputMaterialization, ProjectRunError,
        ProjectRunFailurePolicy, ProjectRunHashRef, ProjectRunNextAction, ProjectRunNodeOutcome,
        ProjectRunNodeReceipt, ProjectRunOutputReceipt, ProjectRunPolicy, ProjectRunReport,
        compile_extension_project_plan, digest_bytes, read_node_receipt, run_project_plan,
        validate_project_plan,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

pub const CANON_GEO_RUN_VERSION: &str = "canon_geo_run.v0";
pub const CANON_GEO_RUN_PROGRESS_VERSION: &str = "canon_geo_run_progress.v0";
pub const GEO_RUN_JSON_MEDIA_TYPE: &str = "application/json";

pub type GeoRunResult<T> = Result<T, GeoRunError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeoRunErrorCode {
    InvalidInput,
    UnsupportedVersion,
    ArtifactContract,
    InputDigestMismatch,
    MissingInput,
    OutputContractViolation,
    ProjectRunFailed,
    ProgressOutput,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunError {
    pub code: GeoRunErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub detail: BTreeMap<String, String>,
}

impl GeoRunError {
    pub fn new(
        code: GeoRunErrorCode,
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
}

impl fmt::Display for GeoRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoRunError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeoRunStatus {
    Completed,
    Partial,
    WaitingForInput,
    UnsupportedGrain,
    Failed,
    Cancelled,
    BudgetFallback,
    Abstained,
    Contradicted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeoRunPhase {
    Drafted,
    Preflighted,
    Materialized,
    ReachChecked,
    Compiled,
    Factorized,
    Solved,
    Reconciled,
    Evaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeoRunProgressEventKind {
    RunStarted,
    StageStarted,
    ArtifactCommitted,
    ArtifactResumed,
    WaitingForInput,
    RunCancelled,
    RunFailed,
    RunFinished,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunProgressCounters {
    pub started_nodes: u64,
    pub completed_nodes: u64,
    pub executed_nodes: u64,
    pub resumed_nodes: u64,
    pub cancelled_nodes: u64,
    pub blocked_nodes: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deterministic_usage: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunProgressArtifactRef {
    pub artifact_id: String,
    pub content_digest: String,
    pub byte_count: u64,
}

/// Non-semantic, deterministic progress emitted only by an opt-in run API.
///
/// Events intentionally omit clocks, paths, worker identity, and telemetry. They are
/// operational observations and never enter `canon_geo_run.v0` or its semantic hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunProgressEvent {
    pub version: String,
    pub sequence: u64,
    pub kind: GeoRunProgressEventKind,
    pub plan_id: String,
    pub project_graph_hash: String,
    pub phase: GeoRunPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<GeoRunStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<GeoPlanStage>,
    pub counters: GeoRunProgressCounters,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_committed_artifact: Option<GeoRunProgressArtifactRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeoRunBlockerKind {
    WaitingForInput,
    UnsupportedGrain,
    MissingLeafCapability,
    ProjectFailure,
    ProjectCancelled,
    ProjectBlocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeoRunNextActionKind {
    SatisfyAcquisition,
    SatisfyDiscovery,
    SupplyLocalArtifact,
    ExecuteProjectNode,
    InspectFailure,
    Resume,
    UnsupportedGrain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoRunRequest {
    pub plan: GeoPlan,
    pub policy: ProjectRunPolicy,
    pub input_bindings: Vec<GeoRunArtifactBinding>,
    pub observation: GeoRunObservation,
}

impl GeoRunRequest {
    pub fn new(
        plan: GeoPlan,
        policy: ProjectRunPolicy,
        input_bindings: Vec<GeoRunArtifactBinding>,
    ) -> Self {
        Self {
            plan,
            policy,
            input_bindings,
            observation: GeoRunObservation::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoRunArtifactBinding {
    pub node_id: String,
    pub binding_id: String,
    pub artifact_id: String,
    pub content_digest: String,
    pub media_type: String,
    pub contract_version: String,
    pub byte_count: u64,
    pub local_path: Option<String>,
    pub bytes: Vec<u8>,
}

pub type GeoRunInputBinding = GeoRunArtifactBinding;

impl GeoRunArtifactBinding {
    pub fn from_json<T: Serialize>(
        node_id: impl Into<String>,
        binding_id: impl Into<String>,
        contract_version: impl Into<String>,
        value: &T,
    ) -> Result<Self, serde_json::Error> {
        let bytes = serde_json::to_vec(value)?;
        Ok(Self::from_bytes(
            node_id,
            binding_id,
            contract_version,
            bytes,
        ))
    }

    pub fn from_bytes(
        node_id: impl Into<String>,
        binding_id: impl Into<String>,
        contract_version: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Self {
        let node_id = node_id.into();
        let binding_id = binding_id.into();
        let byte_count = bytes.len() as u64;
        Self {
            artifact_id: geo_run_input_artifact_id(&node_id, &binding_id),
            content_digest: digest_bytes(&bytes),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            contract_version: contract_version.into(),
            byte_count,
            local_path: None,
            bytes,
            node_id,
            binding_id,
        }
    }

    pub fn with_local_path(mut self, local_path: impl Into<String>) -> Self {
        self.local_path = Some(local_path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunArtifactRef {
    pub node_id: String,
    pub binding_id: String,
    pub artifact_id: String,
    pub content_digest: String,
    pub media_type: String,
    pub contract_version: String,
    pub byte_count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_observations: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunPlanRef {
    pub plan_id: String,
    pub semantic_hash: String,
    pub project_id: String,
    pub project_graph_hash: String,
    pub question_hash: String,
    pub capabilities_hash: String,
    pub inventory_planning_hash: String,
    pub profile_hash: String,
    pub budget_planning_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunGrainState {
    pub entity_level: String,
    pub status: GeoPlanGrainStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_evidence_classes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_node_ids: Vec<String>,
    pub claim_limitation: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunBlocker {
    pub blocker_id: String,
    pub kind: GeoRunBlockerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_level: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunNextAction {
    pub action_id: String,
    pub kind: GeoRunNextActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_contract: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRunOutputRef {
    pub artifact_id: String,
    pub project_node_id: String,
    pub output_id: String,
    pub content_digest: String,
    pub byte_count: u64,
    pub media_type: String,
    pub contract_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRun {
    pub version: String,
    pub run_id: String,
    pub semantic_hash: String,
    pub status: GeoRunStatus,
    pub phase: GeoRunPhase,
    pub plan_ref: GeoRunPlanRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_inputs: Vec<GeoRunArtifactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_refs: Vec<GeoRunOutputRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grain_states: Vec<GeoRunGrainState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<GeoRunBlocker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<GeoRunNextAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deterministic_usage: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_run_report: Option<ProjectRunReport>,
    pub observation: GeoRunObservation,
}

struct GeoRunProgressWriter<'a> {
    writer: Option<&'a mut dyn Write>,
    plan_id: String,
    project_graph_hash: String,
    sequence: u64,
    current_phase: GeoRunPhase,
    counters: GeoRunProgressCounters,
    last_committed_artifact: Option<GeoRunProgressArtifactRef>,
    committed_nodes: BTreeSet<String>,
    delivery_error: Option<String>,
}

impl<'a> GeoRunProgressWriter<'a> {
    fn new(plan: &GeoPlan, project_graph_hash: &str, writer: Option<&'a mut dyn Write>) -> Self {
        Self {
            writer,
            plan_id: plan.plan_id.clone(),
            project_graph_hash: project_graph_hash.to_string(),
            sequence: 0,
            current_phase: GeoRunPhase::Preflighted,
            counters: GeoRunProgressCounters::default(),
            last_committed_artifact: None,
            committed_nodes: BTreeSet::new(),
            delivery_error: None,
        }
    }

    fn run_started(&mut self) {
        self.emit(
            GeoRunProgressEventKind::RunStarted,
            GeoRunPhase::Preflighted,
            None,
            None,
            None,
            None,
        );
    }

    fn stage_started(&mut self, node_id: &str, stage: GeoPlanStage) {
        self.counters.started_nodes += 1;
        self.emit(
            GeoRunProgressEventKind::StageStarted,
            phase_for_stage(stage),
            None,
            Some(node_id),
            Some(stage),
            None,
        );
    }

    fn artifact_committed(
        &mut self,
        node_id: &str,
        stage: GeoPlanStage,
        artifact: GeoRunProgressArtifactRef,
        deterministic_usage: &BTreeMap<String, u64>,
        resumed: bool,
    ) {
        if !self.committed_nodes.insert(node_id.to_string()) {
            return;
        }
        self.counters.completed_nodes += 1;
        if resumed {
            self.counters.resumed_nodes += 1;
        } else {
            self.counters.executed_nodes += 1;
        }
        for (counter, value) in deterministic_usage {
            *self
                .counters
                .deterministic_usage
                .entry(counter.clone())
                .or_insert(0) += value;
        }
        self.last_committed_artifact = Some(artifact);
        self.emit(
            if resumed {
                GeoRunProgressEventKind::ArtifactResumed
            } else {
                GeoRunProgressEventKind::ArtifactCommitted
            },
            phase_for_stage(stage),
            None,
            Some(node_id),
            Some(stage),
            None,
        );
    }

    fn resumed_receipts(
        &mut self,
        project_plan: &ProjectPlan,
        selected_nodes: &BTreeSet<String>,
        receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
        geo_plan: &GeoPlan,
    ) {
        let Some(target_nodes) = project_target_node_ids(project_plan, selected_nodes) else {
            return;
        };
        let stages = geo_plan
            .geo_nodes
            .iter()
            .map(|node| (node.project_node_id.as_str(), node.stage))
            .collect::<BTreeMap<_, _>>();
        for node_id in project_node_ids_in_dependency_order(project_plan) {
            if !target_nodes.contains(&node_id) {
                continue;
            }
            let (Some(receipt), Some(stage)) = (
                receipts.get(&node_id),
                stages.get(node_id.as_str()).copied(),
            ) else {
                continue;
            };
            let Some(output) = receipt.outputs.first() else {
                continue;
            };
            self.artifact_committed(
                &node_id,
                stage,
                GeoRunProgressArtifactRef {
                    artifact_id: geo_run_declared_artifact_id(&node_id, &output.output_id),
                    content_digest: output.content_digest.clone(),
                    byte_count: output.byte_count,
                },
                &receipt.deterministic_usage,
                true,
            );
        }
    }

    fn terminal(
        &mut self,
        kind: GeoRunProgressEventKind,
        run: &GeoRun,
        project_node_id: Option<&str>,
        stage: Option<GeoPlanStage>,
        wait_reason: Option<String>,
    ) {
        if let Some(report) = &run.project_run_report {
            self.counters.cancelled_nodes = report.cancelled_nodes.len() as u64;
            self.counters.blocked_nodes = report.blocked_nodes.len() as u64;
        } else if run.status == GeoRunStatus::WaitingForInput {
            self.counters.blocked_nodes = run
                .blockers
                .iter()
                .filter(|blocker| blocker.kind == GeoRunBlockerKind::WaitingForInput)
                .count() as u64;
        }
        self.emit(
            kind,
            run.phase,
            Some(run.status),
            project_node_id,
            stage,
            wait_reason,
        );
    }

    fn emit(
        &mut self,
        kind: GeoRunProgressEventKind,
        phase: GeoRunPhase,
        status: Option<GeoRunStatus>,
        project_node_id: Option<&str>,
        stage: Option<GeoPlanStage>,
        wait_reason: Option<String>,
    ) {
        if self.writer.is_none() || self.delivery_error.is_some() {
            return;
        }
        self.current_phase = self.current_phase.max(phase);
        let event = GeoRunProgressEvent {
            version: CANON_GEO_RUN_PROGRESS_VERSION.to_string(),
            sequence: self.sequence,
            kind,
            plan_id: self.plan_id.clone(),
            project_graph_hash: self.project_graph_hash.clone(),
            phase: self.current_phase,
            status,
            project_node_id: project_node_id.map(str::to_string),
            stage,
            counters: self.counters.clone(),
            last_committed_artifact: self.last_committed_artifact.clone(),
            wait_reason,
        };
        self.sequence += 1;
        let writer = self.writer.as_mut().expect("checked progress writer");
        let delivery = serde_json::to_writer(&mut **writer, &event)
            .map_err(|error| error.to_string())
            .and_then(|()| writer.write_all(b"\n").map_err(|error| error.to_string()))
            .and_then(|()| writer.flush().map_err(|error| error.to_string()));
        if let Err(error) = delivery {
            self.delivery_error = Some(error);
        }
    }

    fn finish_delivery(&self) -> GeoRunResult<()> {
        if let Some(error) = &self.delivery_error {
            return Err(GeoRunError::new(
                GeoRunErrorCode::ProgressOutput,
                "Geo run completed its semantic work but could not deliver the requested progress stream",
                [("error", error.as_str())],
            ));
        }
        Ok(())
    }
}

struct PendingGeoProgressCommit {
    node_id: String,
    stage: GeoPlanStage,
    artifact: GeoRunProgressArtifactRef,
    deterministic_usage: BTreeMap<String, u64>,
}

struct GeoProgressProjectExecutor<'executor, 'progress, 'writer, E> {
    inner: &'executor mut E,
    progress: &'progress mut GeoRunProgressWriter<'writer>,
    stages: BTreeMap<String, GeoPlanStage>,
    pending_commit: Option<PendingGeoProgressCommit>,
}

impl<'executor, 'progress, 'writer, E>
    GeoProgressProjectExecutor<'executor, 'progress, 'writer, E>
{
    fn new(
        inner: &'executor mut E,
        plan: &GeoPlan,
        progress: &'progress mut GeoRunProgressWriter<'writer>,
    ) -> Self {
        Self {
            inner,
            progress,
            stages: plan
                .geo_nodes
                .iter()
                .map(|node| (node.project_node_id.clone(), node.stage))
                .collect(),
            pending_commit: None,
        }
    }

    fn confirm_pending_commit(&mut self) {
        if let Some(pending) = self.pending_commit.take() {
            self.progress.artifact_committed(
                &pending.node_id,
                pending.stage,
                pending.artifact,
                &pending.deterministic_usage,
                false,
            );
        }
    }

    fn reconcile_report(&mut self, plan: &ProjectPlan, report: &ProjectRunReport) {
        let receipts = report
            .receipt
            .node_receipts
            .iter()
            .map(|receipt| (receipt.node_id.as_str(), receipt))
            .collect::<BTreeMap<_, _>>();
        let resumed = report.resumed_nodes.iter().collect::<BTreeSet<_>>();
        for node_id in project_node_ids_in_dependency_order(plan) {
            if self.progress.committed_nodes.contains(&node_id) {
                continue;
            }
            let Some(receipt) = receipts.get(node_id.as_str()) else {
                continue;
            };
            let Some(stage) = self.stages.get(&node_id).copied() else {
                continue;
            };
            let Some(output) = receipt.outputs.first() else {
                continue;
            };
            self.progress.artifact_committed(
                &node_id,
                stage,
                GeoRunProgressArtifactRef {
                    artifact_id: geo_run_declared_artifact_id(&node_id, &output.output_id),
                    content_digest: output.content_digest.clone(),
                    byte_count: output.byte_count,
                },
                &receipt.deterministic_usage,
                resumed.contains(&node_id),
            );
        }
        self.pending_commit = None;
    }
}

impl<E: ProjectNodeExecutor> ProjectNodeExecutor for GeoProgressProjectExecutor<'_, '_, '_, E> {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &crate::project::ProjectNodeExecutionContext,
    ) -> Result<crate::project::ProjectNodeExecutionResult, ProjectRunError> {
        self.confirm_pending_commit();
        let stage = self.stages.get(&node.node_id).copied().ok_or_else(|| {
            ProjectRunError::new(
                crate::project::ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                "Geo progress could not map a project node to its Geo stage",
            )
        })?;
        self.progress.stage_started(&node.node_id, stage);
        let result = self.inner.execute(node, context)?;
        let output_id = output_id_for_command(&node.command).ok_or_else(|| {
            ProjectRunError::new(
                crate::project::ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                "Geo progress reached a command without a declared output",
            )
        })?;
        let output = result.outputs.get(output_id).ok_or_else(|| {
            ProjectRunError::new(
                crate::project::ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                "Geo progress reached an execution result without its declared output",
            )
        })?;
        self.pending_commit = Some(PendingGeoProgressCommit {
            node_id: node.node_id.clone(),
            stage,
            artifact: GeoRunProgressArtifactRef {
                artifact_id: geo_run_declared_artifact_id(&node.node_id, output_id),
                content_digest: digest_bytes(output),
                byte_count: output.len() as u64,
            },
            deterministic_usage: result.deterministic_usage.clone(),
        });
        Ok(result)
    }
}

pub fn run_geo_plan(request: GeoRunRequest) -> GeoRunResult<GeoRun> {
    let mut executor = GeoProjectNodeExecutor::new();
    run_geo_plan_with_executor(request, &mut executor)
}

pub fn run_geo_plan_with_progress_writer(
    request: GeoRunRequest,
    writer: &mut dyn Write,
) -> GeoRunResult<GeoRun> {
    let mut executor = GeoProjectNodeExecutor::new();
    run_geo_plan_with_geo_executor(request, &mut executor, Some(writer))
}

pub fn run_geo_plan_with_executor(
    request: GeoRunRequest,
    executor: &mut GeoProjectNodeExecutor,
) -> GeoRunResult<GeoRun> {
    run_geo_plan_with_geo_executor(request, executor, None)
}

fn run_geo_plan_with_geo_executor(
    request: GeoRunRequest,
    executor: &mut GeoProjectNodeExecutor,
    writer: Option<&mut dyn Write>,
) -> GeoRunResult<GeoRun> {
    let prepared = prepare_geo_run(
        &request.plan,
        &request.policy,
        request.input_bindings.clone(),
    )?;
    executor.bind_geo_plan(&request.plan);
    for binding in prepared.input_bindings.values() {
        executor.insert_input_binding(geo_executor_input_binding(binding));
    }
    let reusable_receipts = if prepared.missing_inputs.is_empty() {
        seed_geo_project_executor_from_valid_receipts(
            &prepared.effective_project_plan,
            &request.policy,
            executor,
        )?
    } else {
        BTreeMap::new()
    };
    let mut progress = GeoRunProgressWriter::new(
        &request.plan,
        &prepared.effective_project_plan.graph_hash,
        writer,
    );
    execute_prepared_geo_run(
        request,
        prepared,
        reusable_receipts,
        executor,
        &mut progress,
    )
}

pub fn run_geo_plan_with_project_executor<E: ProjectNodeExecutor>(
    request: GeoRunRequest,
    executor: &mut E,
) -> GeoRunResult<GeoRun> {
    let prepared = prepare_geo_run(
        &request.plan,
        &request.policy,
        request.input_bindings.clone(),
    )?;
    let mut validating_executor = GeoContractValidatingExecutor { inner: executor };
    let mut progress = GeoRunProgressWriter::new(
        &request.plan,
        &prepared.effective_project_plan.graph_hash,
        None,
    );
    execute_prepared_geo_run(
        request,
        prepared,
        BTreeMap::new(),
        &mut validating_executor,
        &mut progress,
    )
}

struct GeoContractValidatingExecutor<'a, E> {
    inner: &'a mut E,
}

impl<E: ProjectNodeExecutor> ProjectNodeExecutor for GeoContractValidatingExecutor<'_, E> {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &crate::project::ProjectNodeExecutionContext,
    ) -> Result<crate::project::ProjectNodeExecutionResult, ProjectRunError> {
        let result = self.inner.execute(node, context)?;
        let output_id = output_id_for_command(&node.command).ok_or_else(|| {
            ProjectRunError::new(
                crate::project::ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                "Geo run injected executor reached an unknown command",
            )
        })?;
        let contract = output_contract_for_command(&node.command).ok_or_else(|| {
            ProjectRunError::new(
                crate::project::ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                "Geo run injected executor reached a command without an output contract",
            )
        })?;
        if result.outputs.len() != 1 {
            return Err(ProjectRunError::new(
                crate::project::ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                "Geo run injected executor must return exactly one command-owned output",
            ));
        }
        let bytes = result.outputs.get(output_id).ok_or_else(|| {
            ProjectRunError::new(
                crate::project::ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                "Geo run injected executor returned the wrong output id",
            )
        })?;
        validate_canonical_geo_artifact_bytes(&node.node_id, contract, bytes)?;
        Ok(result)
    }
}

fn execute_prepared_geo_run<E: ProjectNodeExecutor>(
    request: GeoRunRequest,
    prepared: PreparedGeoRun,
    reusable_receipts: BTreeMap<String, ProjectRunNodeReceipt>,
    executor: &mut E,
    progress: &mut GeoRunProgressWriter<'_>,
) -> GeoRunResult<GeoRun> {
    progress.run_started();
    progress.resumed_receipts(
        &prepared.effective_project_plan,
        &request.policy.selected_nodes,
        &reusable_receipts,
        &request.plan,
    );
    let stages = request
        .plan
        .geo_nodes
        .iter()
        .map(|node| (node.project_node_id.clone(), node.stage))
        .collect::<BTreeMap<_, _>>();
    let result = execute_prepared_geo_run_after_start(request, prepared, executor, progress);
    match result {
        Ok(run) => {
            let terminal_node = run
                .project_run_report
                .as_ref()
                .and_then(|report| {
                    report
                        .cancelled_nodes
                        .first()
                        .or_else(|| report.failed_nodes.first())
                })
                .map(String::as_str)
                .or_else(|| {
                    run.blockers
                        .iter()
                        .find_map(|blocker| blocker.project_node_id.as_deref())
                });
            let terminal_stage = terminal_node.and_then(|node_id| stages.get(node_id).copied());
            let (terminal_kind, wait_reason) = match run.status {
                GeoRunStatus::WaitingForInput => (
                    GeoRunProgressEventKind::WaitingForInput,
                    Some(
                        run.blockers
                            .iter()
                            .filter(|blocker| blocker.kind == GeoRunBlockerKind::WaitingForInput)
                            .map(|blocker| blocker.reason.as_str())
                            .collect::<Vec<_>>()
                            .join("; "),
                    ),
                ),
                GeoRunStatus::Cancelled => (
                    GeoRunProgressEventKind::RunCancelled,
                    Some("cancelled before project node execution".to_string()),
                ),
                GeoRunStatus::Failed => (
                    GeoRunProgressEventKind::RunFailed,
                    run.project_run_report.as_ref().and_then(|report| {
                        report
                            .node_reports
                            .iter()
                            .find(|node| node.outcome == ProjectRunNodeOutcome::Failed)
                            .and_then(|node| node.reason.clone())
                    }),
                ),
                _ => (GeoRunProgressEventKind::RunFinished, None),
            };
            progress.terminal(
                terminal_kind,
                &run,
                terminal_node,
                terminal_stage,
                wait_reason,
            );
            progress.finish_delivery()?;
            Ok(run)
        }
        Err(error) => {
            let terminal_node = error
                .detail
                .get("project_node_id")
                .or_else(|| error.detail.get("node_id"))
                .filter(|node_id| node_id.as_str() != "<none>")
                .map(String::as_str);
            let terminal_stage = terminal_node.and_then(|node_id| stages.get(node_id).copied());
            let terminal_phase = progress.current_phase;
            progress.emit(
                GeoRunProgressEventKind::RunFailed,
                terminal_phase,
                Some(GeoRunStatus::Failed),
                terminal_node,
                terminal_stage,
                Some(error.message.clone()),
            );
            let _ = progress.finish_delivery();
            Err(error)
        }
    }
}

fn execute_prepared_geo_run_after_start<E: ProjectNodeExecutor>(
    request: GeoRunRequest,
    prepared: PreparedGeoRun,
    executor: &mut E,
    progress: &mut GeoRunProgressWriter<'_>,
) -> GeoRunResult<GeoRun> {
    let artifact_inputs = artifact_refs_from_bindings(&prepared.input_bindings);
    let mut preflight_blockers = Vec::new();
    let mut preflight_actions = Vec::new();
    for input in &prepared.missing_inputs {
        preflight_blockers.push(GeoRunBlocker {
            blocker_id: format!("waiting_for_input:{}", input.artifact_id),
            kind: GeoRunBlockerKind::WaitingForInput,
            project_node_id: Some(input.node_id.clone()),
            entity_level: None,
            reason: input.reason.clone(),
        });
        preflight_actions.push(GeoRunNextAction {
            action_id: format!("supply:{}", input.artifact_id),
            kind: GeoRunNextActionKind::SupplyLocalArtifact,
            project_node_id: Some(input.node_id.clone()),
            artifact_id: Some(input.artifact_id.clone()),
            expected_contract: Some(input.accepted_contracts.join("|")),
            media_type: Some(GEO_RUN_JSON_MEDIA_TYPE.to_string()),
            command: None,
            reason: "bind local bytes by project node id, binding id, artifact id, canonical lowercase BLAKE3 digest, media type, and input contract; filesystem paths remain operational".to_string(),
        });
    }

    if !preflight_blockers.is_empty() {
        return build_geo_run(
            request.plan,
            prepared.effective_project_plan.graph_hash,
            artifact_inputs,
            None,
            None,
            GeoRunBuildContext {
                observation: request.observation,
                extra_blockers: preflight_blockers,
                extra_actions: preflight_actions,
            },
        );
    }

    validate_effective_geo_project_nodes(&request.plan, &prepared.effective_project_plan)?;
    let project_report = {
        let mut reporting_executor =
            GeoProgressProjectExecutor::new(executor, &request.plan, progress);
        let report = run_project_plan(
            &prepared.effective_project_plan,
            &request.policy,
            &mut reporting_executor,
        )
        .map_err(project_run_error)?;
        reporting_executor.reconcile_report(&prepared.effective_project_plan, &report);
        report
    };
    validate_geo_project_report_outputs(
        &request.plan,
        &prepared.effective_project_plan,
        &project_report,
        &request.policy,
    )?;
    let composition_status = final_composition_status_from_project_report(
        &prepared.effective_project_plan,
        &project_report,
        &request.policy,
    )?;
    build_geo_run(
        request.plan,
        prepared.effective_project_plan.graph_hash,
        artifact_inputs,
        Some(project_report),
        composition_status,
        GeoRunBuildContext {
            observation: request.observation,
            extra_blockers: Vec::new(),
            extra_actions: Vec::new(),
        },
    )
}

fn validate_geo_project_report_outputs(
    plan: &GeoPlan,
    effective_plan: &ProjectPlan,
    report: &ProjectRunReport,
    policy: &ProjectRunPolicy,
) -> GeoRunResult<()> {
    let nodes = effective_plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let overlays = plan
        .geo_nodes
        .iter()
        .map(|overlay| (overlay.project_node_id.as_str(), overlay))
        .collect::<BTreeMap<_, _>>();
    let mut artifacts = BTreeMap::new();

    for receipt in &report.receipt.node_receipts {
        if receipt.outcome != ProjectRunNodeOutcome::Completed {
            continue;
        }
        let node = nodes.get(receipt.node_id.as_str()).ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo run report contains a completed receipt for an undeclared node",
                [("project_node_id", receipt.node_id.as_str())],
            )
        })?;
        let overlay = overlays.get(receipt.node_id.as_str()).ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo run report contains a completed receipt without a Geo overlay",
                [("project_node_id", receipt.node_id.as_str())],
            )
        })?;
        validate_receipt_outputs_match_effective_node(node, receipt)?;
        for output in &receipt.outputs {
            let bytes = read_receipt_output_bytes(policy, receipt, output)?;
            validate_canonical_geo_artifact_bytes(
                receipt.node_id.as_str(),
                &overlay.expected_output_contract,
                &bytes,
            )
            .map_err(project_run_error)?;
            artifacts.insert((receipt.node_id.clone(), output.output_id.clone()), bytes);
        }
    }

    for overlay in plan
        .geo_nodes
        .iter()
        .filter(|overlay| overlay.stage == GeoPlanStage::FactorAndSolveExactResidual)
    {
        let solve_key = (overlay.project_node_id.clone(), "solve".to_string());
        let Some(solve_bytes) = artifacts.get(&solve_key) else {
            continue;
        };
        let scope = overlay.exact_solve_scope.as_ref().ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "completed Geo solve is missing its exact_solve_scope",
                [("project_node_id", overlay.project_node_id.as_str())],
            )
        })?;
        let section_key = (
            scope.bounded_section.producer_node_id.clone(),
            scope.bounded_section.output_id.clone(),
        );
        let compilation_key = (
            scope.evidence_compilation.producer_node_id.clone(),
            scope.evidence_compilation.output_id.clone(),
        );
        let section_bytes = artifacts.get(&section_key).ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "completed Geo solve exact_solve_scope has no completed bounded-section artifact",
                [("project_node_id", overlay.project_node_id.as_str())],
            )
        })?;
        let compilation_bytes = artifacts.get(&compilation_key).ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "completed Geo solve exact_solve_scope has no completed evidence-compilation artifact",
                [("project_node_id", overlay.project_node_id.as_str())],
            )
        })?;
        validate_solve_artifact_lineage(
            &overlay.project_node_id,
            section_bytes,
            compilation_bytes,
            solve_bytes,
        )?;
    }
    Ok(())
}

fn validate_solve_artifact_lineage(
    solve_node_id: &str,
    section_bytes: &[u8],
    compilation_bytes: &[u8],
    solve_bytes: &[u8],
) -> GeoRunResult<()> {
    let section: GeoTileWorkUnitArtifact = serde_json::from_slice(section_bytes)
        .map_err(|error| output_parse_error(solve_node_id, "bounded section", error))?;
    let compilation: crate::geo::GeoEvidenceCompilationArtifact =
        serde_json::from_slice(compilation_bytes)
            .map_err(|error| output_parse_error(solve_node_id, "evidence compilation", error))?;
    let solve: GeoCompositionArtifact = serde_json::from_slice(solve_bytes)
        .map_err(|error| output_parse_error(solve_node_id, "composition", error))?;

    let section_ids = section
        .features
        .iter()
        .map(|feature| feature.feature_id.as_str())
        .collect::<BTreeSet<_>>();
    let universe_ids = match compilation.composition_request.profile.selection_level {
        crate::geo::GeoEntityLevel::Parcel => compilation
            .composition_request
            .universe
            .parcels
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        crate::geo::GeoEntityLevel::Building => compilation
            .composition_request
            .universe
            .buildings
            .iter()
            .map(|building| building.id.as_str())
            .collect::<BTreeSet<_>>(),
        _ => {
            return Err(GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "completed Geo solve uses an unsupported selected-grain universe",
                [("project_node_id", solve_node_id)],
            ));
        }
    };
    if section_ids.len() != section.features.len() || section_ids != universe_ids {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "completed Geo solve candidate universe does not equal its exact_solve_scope bounded section",
            [("project_node_id", solve_node_id)],
        ));
    }

    let compilation_digest = blake3::hash(compilation_bytes).to_hex().to_string();
    let Some(reference) = solve.evidence_compilation.as_ref() else {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "completed Geo solve does not reference its exact_solve_scope evidence compilation",
            [("project_node_id", solve_node_id)],
        ));
    };
    if reference.version != compilation.version
        || reference.request_version != compilation.request_version
        || reference.blake3 != compilation_digest
    {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "completed Geo solve evidence reference does not match its exact_solve_scope compilation bytes",
            [("project_node_id", solve_node_id)],
        ));
    }
    Ok(())
}

fn output_parse_error(project_node_id: &str, label: &str, error: serde_json::Error) -> GeoRunError {
    GeoRunError::new(
        GeoRunErrorCode::ArtifactContract,
        format!("Geo run could not parse {label} output"),
        [
            ("project_node_id", project_node_id.to_string()),
            ("error", error.to_string()),
        ],
    )
}

pub fn canonical_geo_run_bytes(run: &GeoRun) -> GeoRunResult<Vec<u8>> {
    validate_geo_run(run)?;
    serde_json::to_vec(run).map_err(serialization_error)
}

pub fn canonical_geo_run_semantic_bytes(run: &GeoRun) -> GeoRunResult<Vec<u8>> {
    validate_geo_run(run)?;
    let projection = semantic_projection(run);
    serde_json::to_vec(&projection).map_err(serialization_error)
}

pub fn geo_run_semantic_hash(run: &GeoRun) -> GeoRunResult<String> {
    serde_json::to_vec(&semantic_projection(run))
        .map(|bytes| digest_bytes(&bytes))
        .map_err(serialization_error)
}

pub fn validate_geo_run(run: &GeoRun) -> GeoRunResult<()> {
    if run.version != CANON_GEO_RUN_VERSION {
        return Err(GeoRunError::new(
            GeoRunErrorCode::UnsupportedVersion,
            "unsupported Geo run version",
            [("version", run.version.as_str())],
        ));
    }
    validate_digest("semantic_hash", &run.semantic_hash)?;
    validate_plan_id("plan_ref.plan_id", &run.plan_ref.plan_id)?;
    validate_digest("plan_ref.semantic_hash", &run.plan_ref.semantic_hash)?;
    let expected_plan_id = format!(
        "{CANON_GEO_PLAN_VERSION}:{}",
        run.plan_ref.semantic_hash.trim_start_matches("blake3:")
    );
    if run.plan_ref.plan_id != expected_plan_id {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run plan_ref.plan_id does not match plan_ref.semantic_hash",
            [
                ("expected", expected_plan_id),
                ("actual", run.plan_ref.plan_id.clone()),
            ],
        ));
    }
    validate_local_id("plan_ref.project_id", &run.plan_ref.project_id, 256)?;
    validate_digest(
        "plan_ref.project_graph_hash",
        &run.plan_ref.project_graph_hash,
    )?;
    validate_digest("plan_ref.question_hash", &run.plan_ref.question_hash)?;
    validate_digest(
        "plan_ref.capabilities_hash",
        &run.plan_ref.capabilities_hash,
    )?;
    validate_digest(
        "plan_ref.inventory_planning_hash",
        &run.plan_ref.inventory_planning_hash,
    )?;
    validate_digest("plan_ref.profile_hash", &run.plan_ref.profile_hash)?;
    validate_digest(
        "plan_ref.budget_planning_hash",
        &run.plan_ref.budget_planning_hash,
    )?;
    for input in &run.artifact_inputs {
        validate_artifact_ref(input)?;
    }
    for output in &run.output_refs {
        validate_project_node_id("output_ref.project_node_id", &output.project_node_id)?;
        validate_local_id("output_ref.output_id", &output.output_id, 256)?;
        validate_digest("output_ref.content_digest", &output.content_digest)?;
        validate_json_contract(
            "output_ref.media_type",
            "output_ref.contract_version",
            &output.media_type,
            &output.contract_version,
        )?;
        if output.artifact_id
            != geo_run_declared_artifact_id(&output.project_node_id, &output.output_id)
        {
            return Err(GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo run output artifact id must be derived from project node and output id",
                [("artifact_id", output.artifact_id.as_str())],
            ));
        }
    }
    if let Some(report) = &run.project_run_report {
        validate_project_run_report_schema_shape(report)?;
    }
    validate_run_state_shapes(run)?;
    validate_run_state_invariants(run)?;
    validate_canonical_run_order(run)?;
    let expected_hash = geo_run_semantic_hash(run)?;
    if run.semantic_hash != expected_hash {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run semantic_hash does not match its semantic projection",
            [
                ("expected", expected_hash.as_str()),
                ("actual", run.semantic_hash.as_str()),
            ],
        ));
    }
    let expected_id = format!(
        "{CANON_GEO_RUN_VERSION}:{}",
        expected_hash.trim_start_matches("blake3:")
    );
    if run.run_id != expected_id {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run run_id does not match semantic_hash",
            [("expected", expected_id), ("actual", run.run_id.clone())],
        ));
    }
    Ok(())
}

pub fn geo_run_declared_artifact_id(project_node_id: &str, output_id: &str) -> String {
    format!("{project_node_id}/{output_id}")
}

pub fn geo_run_input_artifact_id(project_node_id: &str, binding_id: &str) -> String {
    format!("{project_node_id}/input/{binding_id}")
}

pub fn geo_run_input_hash_ref_id(project_node_id: &str, binding_id: &str) -> String {
    format!("geo.run.input.{project_node_id}.{binding_id}")
}

fn build_geo_run(
    plan: GeoPlan,
    effective_project_graph_hash: String,
    mut artifact_inputs: Vec<GeoRunArtifactRef>,
    project_run_report: Option<ProjectRunReport>,
    composition_status: Option<GeoCompositionStatus>,
    context: GeoRunBuildContext,
) -> GeoRunResult<GeoRun> {
    artifact_inputs.sort();
    artifact_inputs.dedup();
    let output_refs = project_run_report
        .as_ref()
        .map(|report| output_refs_from_project_report(&plan, report))
        .transpose()?
        .unwrap_or_default();
    let deterministic_usage = aggregate_deterministic_usage(project_run_report.as_ref());
    let mut grain_states = grain_states(&plan);
    let mut blockers = blockers_from_plan(&plan);
    blockers.extend(blockers_from_project_report(project_run_report.as_ref()));
    blockers.extend(context.extra_blockers);
    let mut next_actions = next_actions_from_plan(&plan);
    next_actions.extend(next_actions_from_project_report(
        project_run_report.as_ref(),
    ));
    next_actions.extend(context.extra_actions);
    sort_and_dedup_run_parts(
        &mut grain_states,
        &mut blockers,
        &mut next_actions,
        &mut artifact_inputs,
    );
    let status = run_status(
        &plan,
        project_run_report.as_ref(),
        composition_status,
        &blockers,
        &next_actions,
    );
    let phase = run_phase(&plan, project_run_report.as_ref(), status);
    let plan_ref = GeoRunPlanRef {
        plan_id: plan.plan_id.clone(),
        semantic_hash: plan.semantic_hash.clone(),
        project_id: plan.project_plan.project_id.clone(),
        project_graph_hash: effective_project_graph_hash,
        question_hash: plan.question_ref.semantic_hash,
        capabilities_hash: plan.capabilities_ref.semantic_hash,
        inventory_planning_hash: plan.inventory_ref.planning_hash,
        profile_hash: plan.profile_ref.semantic_hash,
        budget_planning_hash: plan.budget_ref.planning_hash,
    };
    let mut run = GeoRun {
        version: CANON_GEO_RUN_VERSION.to_string(),
        run_id: String::new(),
        semantic_hash: String::new(),
        status,
        phase,
        plan_ref,
        artifact_inputs,
        output_refs,
        grain_states,
        blockers,
        next_actions,
        deterministic_usage,
        project_run_report,
        observation: context.observation,
    };
    run.semantic_hash = geo_run_semantic_hash(&run)?;
    run.run_id = format!(
        "{CANON_GEO_RUN_VERSION}:{}",
        run.semantic_hash.trim_start_matches("blake3:")
    );
    validate_geo_run(&run)?;
    Ok(run)
}

struct GeoRunBuildContext {
    observation: GeoRunObservation,
    extra_blockers: Vec<GeoRunBlocker>,
    extra_actions: Vec<GeoRunNextAction>,
}

#[derive(Debug, Clone)]
struct PreparedGeoRun {
    effective_project_plan: ProjectPlan,
    input_bindings: BTreeMap<(String, String), GeoRunArtifactBinding>,
    missing_inputs: Vec<RequiredGeoInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RequiredGeoInput {
    node_id: String,
    binding_id: String,
    artifact_id: String,
    accepted_contracts: Vec<String>,
    reason: String,
}

#[derive(Debug, Clone, Copy)]
struct GeoInputSpec {
    binding_id: &'static str,
    required: bool,
    accepted_contracts: &'static [&'static str],
    reason: &'static str,
}

fn prepare_geo_run(
    plan: &GeoPlan,
    policy: &ProjectRunPolicy,
    bindings: Vec<GeoRunArtifactBinding>,
) -> GeoRunResult<PreparedGeoRun> {
    validate_geo_plan(plan).map_err(plan_error)?;
    validate_run_policy(policy)?;
    validate_effective_geo_project_nodes(plan, &plan.project_plan)?;
    let input_bindings = normalize_artifact_bindings(policy, bindings)?;
    validate_input_bindings_against_plan(plan, &input_bindings)?;
    let required_inputs = required_geo_inputs(plan)?;
    let missing_inputs = required_inputs
        .into_values()
        .filter(|input| {
            !input_bindings.contains_key(&(input.node_id.clone(), input.binding_id.clone()))
        })
        .collect::<Vec<_>>();
    let effective_project_plan =
        effective_project_plan_with_input_hashes(&plan.project_plan, &input_bindings)?;
    validate_effective_geo_project_nodes(plan, &effective_project_plan)?;
    Ok(PreparedGeoRun {
        effective_project_plan,
        input_bindings,
        missing_inputs,
    })
}

fn validate_effective_geo_project_nodes(
    plan: &GeoPlan,
    project_plan: &ProjectPlan,
) -> GeoRunResult<()> {
    let nodes = project_plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    for overlay in &plan.geo_nodes {
        let node = nodes.get(overlay.project_node_id.as_str()).ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo overlay references a missing project node",
                [("project_node_id", overlay.project_node_id.as_str())],
            )
        })?;
        validate_geo_project_node(node, overlay)?;
    }
    let overlay_ids = plan
        .geo_nodes
        .iter()
        .map(|overlay| overlay.project_node_id.as_str())
        .collect::<BTreeSet<_>>();
    let project_ids = project_plan
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<BTreeSet<_>>();
    if overlay_ids != project_ids {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run requires a one-to-one overlay over the effective project plan",
            [
                (
                    "overlay_nodes",
                    overlay_ids.into_iter().collect::<Vec<_>>().join(","),
                ),
                (
                    "project_nodes",
                    project_ids.into_iter().collect::<Vec<_>>().join(","),
                ),
            ],
        ));
    }
    Ok(())
}

fn validate_geo_project_node(
    node: &ProjectPlanNode,
    overlay: &GeoPlanNodeOverlay,
) -> GeoRunResult<()> {
    let Some(expected_contract) = output_contract_for_command(&node.command) else {
        return Err(GeoRunError::new(
            GeoRunErrorCode::OutputContractViolation,
            "Geo run refuses unknown or undeclared leaf commands",
            [
                ("project_node_id", node.node_id.as_str()),
                ("command", node.command.as_str()),
            ],
        ));
    };
    if expected_contract != overlay.expected_output_contract {
        return Err(GeoRunError::new(
            GeoRunErrorCode::OutputContractViolation,
            "Geo node command output contract does not match the overlay contract",
            [
                ("project_node_id", node.node_id.as_str()),
                ("command", node.command.as_str()),
                ("expected", expected_contract),
                ("actual", overlay.expected_output_contract.as_str()),
            ],
        ));
    }
    let expected_output_id = output_id_for_command(&node.command).expect("known command output id");
    if node.outputs.len() != 1 || node.outputs[0].output_id != expected_output_id {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run nodes must declare exactly the output id owned by their leaf command",
            [
                ("project_node_id", node.node_id.as_str()),
                ("expected_output_id", expected_output_id),
            ],
        ));
    }
    for output in &node.outputs {
        if output.materialization != ProjectPlanOutputMaterialization::PlannedArtifact {
            return Err(GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo run can execute only planned artifact outputs",
                [
                    ("project_node_id", node.node_id.as_str()),
                    ("output_id", output.output_id.as_str()),
                ],
            ));
        }
    }
    Ok(())
}

fn output_contract_for_command(command: &str) -> Option<&'static str> {
    match command {
        GEO_MATERIALIZE_HOME_CELLS_COMMAND => Some(CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION),
        GEO_TILE_WORK_COMMAND => Some(CANON_GEO_TILE_WORK_UNIT_VERSION),
        GEO_MATERIALIZE_EVIDENCE_COMMAND => Some(CANON_GEO_EVIDENCE_REQUEST_VERSION),
        GEO_COMPILE_EVIDENCE_COMMAND => Some(CANON_GEO_EVIDENCE_COMPILATION_VERSION),
        GEO_SOLVE_COMMAND => Some(CANON_GEO_COMPOSITION_VERSION),
        _ => None,
    }
}

fn output_id_for_command(command: &str) -> Option<&'static str> {
    match command {
        GEO_MATERIALIZE_HOME_CELLS_COMMAND => Some("home_cells"),
        GEO_TILE_WORK_COMMAND => Some("section"),
        GEO_MATERIALIZE_EVIDENCE_COMMAND => Some("materialize_evidence"),
        GEO_COMPILE_EVIDENCE_COMMAND => Some("compile_evidence"),
        GEO_SOLVE_COMMAND => Some("solve"),
        _ => None,
    }
}

fn output_contract_for_output_id(output_id: &str) -> Option<&'static str> {
    match output_id {
        "home_cells" => Some(CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION),
        "section" => Some(CANON_GEO_TILE_WORK_UNIT_VERSION),
        "materialize_evidence" => Some(CANON_GEO_EVIDENCE_REQUEST_VERSION),
        "compile_evidence" => Some(CANON_GEO_EVIDENCE_COMPILATION_VERSION),
        "solve" => Some(CANON_GEO_COMPOSITION_VERSION),
        _ => None,
    }
}

fn input_specs_for_command(command: &str) -> Option<Vec<GeoInputSpec>> {
    match command {
        GEO_MATERIALIZE_HOME_CELLS_COMMAND => Some(vec![GeoInputSpec {
            binding_id: GEO_ROWS_BINDING_ID,
            required: true,
            accepted_contracts: &[CANON_GEO_HOME_CELL_ROWS_VERSION],
            reason: "materialize-home-cells requires local typed home-cell rows",
        }]),
        GEO_TILE_WORK_COMMAND => Some(vec![GeoInputSpec {
            binding_id: GEO_REQUEST_BINDING_ID,
            required: true,
            accepted_contracts: &[CANON_GEO_TILE_WORK_REQUEST_VERSION],
            reason: "tile-work requires a local typed bounded-section request",
        }]),
        GEO_MATERIALIZE_EVIDENCE_COMMAND => Some(vec![GeoInputSpec {
            binding_id: GEO_ROWS_BINDING_ID,
            required: true,
            accepted_contracts: &[CANON_GEO_WAREHOUSE_ROWS_VERSION],
            reason: "materialize-evidence requires local typed warehouse rows",
        }]),
        GEO_COMPILE_EVIDENCE_COMMAND | GEO_SOLVE_COMMAND => Some(Vec::new()),
        _ => None,
    }
}

fn required_geo_inputs(
    plan: &GeoPlan,
) -> GeoRunResult<BTreeMap<(String, String), RequiredGeoInput>> {
    let mut inputs = BTreeMap::new();
    for node in &plan.project_plan.nodes {
        let Some(specs) = input_specs_for_command(&node.command) else {
            return Err(GeoRunError::new(
                GeoRunErrorCode::OutputContractViolation,
                "Geo run refuses unknown or undeclared leaf commands",
                [
                    ("project_node_id", node.node_id.as_str()),
                    ("command", node.command.as_str()),
                ],
            ));
        };
        for spec in specs.into_iter().filter(|spec| spec.required) {
            let input = RequiredGeoInput {
                node_id: node.node_id.clone(),
                binding_id: spec.binding_id.to_string(),
                artifact_id: geo_run_input_artifact_id(&node.node_id, spec.binding_id),
                accepted_contracts: spec
                    .accepted_contracts
                    .iter()
                    .map(|contract| (*contract).to_string())
                    .collect(),
                reason: spec.reason.to_string(),
            };
            if inputs
                .insert((input.node_id.clone(), input.binding_id.clone()), input)
                .is_some()
            {
                return Err(GeoRunError::new(
                    GeoRunErrorCode::ArtifactContract,
                    "duplicate Geo run required input binding",
                    [("project_node_id", node.node_id.as_str())],
                ));
            }
        }
    }
    Ok(inputs)
}

fn normalize_artifact_bindings(
    policy: &ProjectRunPolicy,
    bindings: Vec<GeoRunArtifactBinding>,
) -> GeoRunResult<BTreeMap<(String, String), GeoRunArtifactBinding>> {
    let mut by_id = BTreeMap::new();
    for mut binding in bindings {
        if let Some(local_path) = binding.local_path.clone() {
            let local_bytes = read_local_binding_bytes(policy, &local_path)?;
            if binding.bytes.is_empty() {
                binding.byte_count = local_bytes.len() as u64;
                binding.bytes = local_bytes;
            } else if binding.bytes != local_bytes {
                return Err(GeoRunError::new(
                    GeoRunErrorCode::InputDigestMismatch,
                    "Geo run local binding path bytes do not match the supplied binding bytes",
                    [
                        ("artifact_id", binding.artifact_id.as_str()),
                        ("local_path", local_path.as_str()),
                    ],
                ));
            }
        }
        validate_artifact_binding(&binding)?;
        let key = (binding.node_id.clone(), binding.binding_id.clone());
        if by_id.insert(key.clone(), binding.clone()).is_some() {
            return Err(GeoRunError::new(
                GeoRunErrorCode::InvalidInput,
                "duplicate Geo run artifact binding",
                [
                    ("project_node_id", key.0),
                    ("binding_id", key.1),
                    ("artifact_id", binding.artifact_id),
                ],
            ));
        }
    }
    Ok(by_id)
}

fn validate_artifact_binding(binding: &GeoRunArtifactBinding) -> GeoRunResult<()> {
    if binding.node_id.trim().is_empty()
        || binding.node_id.trim() != binding.node_id
        || binding.binding_id.trim().is_empty()
        || binding.binding_id.trim() != binding.binding_id
        || binding.artifact_id.trim().is_empty()
        || binding.artifact_id.trim() != binding.artifact_id
        || binding.media_type.trim().is_empty()
        || binding.media_type.trim() != binding.media_type
        || binding.contract_version.trim().is_empty()
        || binding.contract_version.trim() != binding.contract_version
    {
        return Err(GeoRunError::new(
            GeoRunErrorCode::InvalidInput,
            "Geo run artifact bindings require non-empty trimmed artifact id, media type, and contract",
            [("artifact_id", binding.artifact_id.as_str())],
        ));
    }
    let expected_artifact_id = geo_run_input_artifact_id(&binding.node_id, &binding.binding_id);
    if binding.artifact_id != expected_artifact_id {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run input artifact id must be derived from project node and binding id",
            [
                ("expected", expected_artifact_id),
                ("actual", binding.artifact_id.clone()),
            ],
        ));
    }
    if binding.media_type != GEO_RUN_JSON_MEDIA_TYPE {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run currently accepts only JSON input artifacts",
            [
                ("artifact_id", binding.artifact_id.as_str()),
                ("media_type", binding.media_type.as_str()),
            ],
        ));
    }
    validate_digest("artifact_binding.content_digest", &binding.content_digest)?;
    if binding.byte_count != binding.bytes.len() as u64 {
        return Err(GeoRunError::new(
            GeoRunErrorCode::InputDigestMismatch,
            "Geo run artifact byte_count does not match bound bytes",
            [
                ("artifact_id", binding.artifact_id.clone()),
                ("declared_byte_count", binding.byte_count.to_string()),
                ("actual_byte_count", binding.bytes.len().to_string()),
            ],
        ));
    }
    let actual_digest = digest_bytes(&binding.bytes);
    if actual_digest != binding.content_digest {
        return Err(GeoRunError::new(
            GeoRunErrorCode::InputDigestMismatch,
            "Geo run artifact bytes do not match the declared BLAKE3 digest",
            [
                ("artifact_id", binding.artifact_id.clone()),
                ("expected", binding.content_digest.clone()),
                ("actual", actual_digest),
            ],
        ));
    }
    validate_json_contract_bytes(
        &binding.artifact_id,
        &binding.contract_version,
        &binding.bytes,
    )
}

fn validate_json_contract_bytes(
    artifact_id: &str,
    expected_contract: &str,
    bytes: &[u8],
) -> GeoRunResult<()> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        GeoRunError::new(
            GeoRunErrorCode::OutputContractViolation,
            "Geo run artifact is not valid JSON for its declared contract",
            [
                ("artifact_id", artifact_id.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    if value.get("version").and_then(serde_json::Value::as_str) != Some(expected_contract) {
        return Err(GeoRunError::new(
            GeoRunErrorCode::OutputContractViolation,
            "Geo run artifact JSON version does not match its declared contract",
            [
                ("artifact_id", artifact_id.to_string()),
                ("expected", expected_contract.to_string()),
            ],
        ));
    }
    Ok(())
}

fn read_local_binding_bytes(policy: &ProjectRunPolicy, local_path: &str) -> GeoRunResult<Vec<u8>> {
    let path = resolve_workspace_path(
        &policy.workspace_root,
        "geo_run.input_binding",
        Path::new(local_path),
        PlannedAccess::Read,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        GeoRunError::new(
            GeoRunErrorCode::InvalidInput,
            "Geo run input binding path failed workspace safety",
            [
                ("local_path", local_path.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    fs::read(&path).map_err(|error| {
        GeoRunError::new(
            GeoRunErrorCode::InvalidInput,
            "Geo run could not read input binding path",
            [
                ("local_path", local_path.to_string()),
                ("error", error.to_string()),
            ],
        )
    })
}

fn validate_input_bindings_against_plan(
    plan: &GeoPlan,
    bindings: &BTreeMap<(String, String), GeoRunArtifactBinding>,
) -> GeoRunResult<()> {
    let nodes = plan
        .project_plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    for binding in bindings.values() {
        let Some(node) = nodes.get(binding.node_id.as_str()) else {
            return Err(GeoRunError::new(
                GeoRunErrorCode::MissingInput,
                "Geo run input binding targets a node that is not in the plan",
                [
                    ("project_node_id", binding.node_id.as_str()),
                    ("binding_id", binding.binding_id.as_str()),
                ],
            ));
        };
        let Some(specs) = input_specs_for_command(&node.command) else {
            return Err(GeoRunError::new(
                GeoRunErrorCode::OutputContractViolation,
                "Geo run refuses unknown or undeclared leaf commands",
                [
                    ("project_node_id", node.node_id.as_str()),
                    ("command", node.command.as_str()),
                ],
            ));
        };
        let Some(spec) = specs
            .iter()
            .find(|spec| spec.binding_id == binding.binding_id.as_str())
        else {
            return Err(GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo run input binding id is not declared for this node command",
                [
                    ("project_node_id", binding.node_id.as_str()),
                    ("binding_id", binding.binding_id.as_str()),
                    ("command", node.command.as_str()),
                ],
            ));
        };
        if !spec
            .accepted_contracts
            .contains(&binding.contract_version.as_str())
        {
            return Err(GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo run input binding contract does not match the node command",
                [
                    ("project_node_id", binding.node_id.clone()),
                    ("binding_id", binding.binding_id.clone()),
                    ("expected", spec.accepted_contracts.join("|")),
                    ("actual", binding.contract_version.clone()),
                ],
            ));
        }
    }
    Ok(())
}

fn effective_project_plan_with_input_hashes(
    project_plan: &ProjectPlan,
    bindings: &BTreeMap<(String, String), GeoRunArtifactBinding>,
) -> GeoRunResult<ProjectPlan> {
    let mut extension_nodes = Vec::with_capacity(project_plan.nodes.len());
    for node in &project_plan.nodes {
        let mut content_hash_inputs = node.content_hash_inputs.clone();
        let dependency_refs =
            node.dependencies
                .iter()
                .flat_map(|dependency_id| {
                    project_plan
                        .nodes
                        .iter()
                        .find(|candidate| candidate.node_id == *dependency_id)
                        .into_iter()
                        .flat_map(move |dependency| {
                            dependency.outputs.iter().map(move |output| {
                                format!("node.{dependency_id}.{}", output.output_id)
                            })
                        })
                })
                .collect::<BTreeSet<_>>();
        content_hash_inputs.retain(|input| !dependency_refs.contains(&input.ref_id));
        for binding in bindings
            .values()
            .filter(|binding| binding.node_id == node.node_id)
        {
            add_input_hash_ref(&mut content_hash_inputs, binding)?;
        }
        extension_nodes.push(ProjectExtensionDagNode {
            node_id: node.node_id.clone(),
            kind: node.kind,
            class: node.class,
            command: node.command.clone(),
            dependencies: node.dependencies.clone(),
            content_hash_inputs,
            outputs: node
                .outputs
                .iter()
                .map(|output| ProjectExtensionDagOutput {
                    output_id: output.output_id.clone(),
                    path: output.path.clone(),
                    materialization: output.materialization,
                })
                .collect(),
            limits: node.limits.clone(),
            cache_eligible: node.cache.eligible,
            side_effects: node.side_effects.clone(),
            refusal_conditions: node.refusal_conditions.clone(),
        });
    }
    let mut request = ProjectExtensionDagRequest::offline_read_only(
        project_plan.project_id.clone(),
        project_plan.manifest_digest.clone(),
        project_plan.lock_digest.clone(),
        extension_nodes,
    );
    request.plan_artifact_path = project_plan.plan_artifact_path.clone();
    request.cache_hits = project_plan
        .nodes
        .iter()
        .filter(|node| node.cache.decision == ProjectPlanCacheDecision::Hit)
        .map(|node| node.node_id.clone())
        .collect();
    let plan = compile_extension_project_plan(request).map_err(|error| {
        GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run could not construct the effective project DAG",
            [
                ("project_error", format!("{:?}", error.code)),
                ("message", error.message),
            ],
        )
    })?;
    validate_project_plan(&plan).map_err(|error| {
        GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run effective project DAG failed validation",
            [
                ("project_error", format!("{:?}", error.code)),
                ("message", error.message),
            ],
        )
    })?;
    Ok(plan)
}

fn add_input_hash_ref(
    inputs: &mut Vec<ProjectPlanHashRef>,
    binding: &GeoRunArtifactBinding,
) -> GeoRunResult<()> {
    let ref_id = geo_run_input_hash_ref_id(&binding.node_id, &binding.binding_id);
    if let Some(existing) = inputs.iter().find(|input| input.ref_id == ref_id) {
        if existing.content_hash == binding.content_digest {
            return Ok(());
        }
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run input hash ref conflicts with an existing project content input",
            [
                ("ref_id", ref_id),
                ("existing", existing.content_hash.clone()),
                ("actual", binding.content_digest.clone()),
            ],
        ));
    }
    inputs.push(ProjectPlanHashRef {
        ref_id,
        content_hash: binding.content_digest.clone(),
    });
    inputs.sort_by(|left, right| left.ref_id.cmp(&right.ref_id));
    Ok(())
}

fn geo_executor_input_binding(binding: &GeoRunArtifactBinding) -> GeoExecutorInputBinding {
    GeoExecutorInputBinding {
        node_id: binding.node_id.clone(),
        binding_id: binding.binding_id.clone(),
        contract: binding.contract_version.clone(),
        content_hash: binding.content_digest.clone(),
        bytes: binding.bytes.clone(),
    }
}

fn seed_geo_project_executor_from_valid_receipts(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    executor: &mut GeoProjectNodeExecutor,
) -> GeoRunResult<BTreeMap<String, ProjectRunNodeReceipt>> {
    let mut candidate_receipts = BTreeMap::new();
    for node in &plan.nodes {
        let receipt_path = project_receipt_path(policy, &node.node_id)?;
        if !receipt_path.exists() {
            continue;
        }
        let receipt = read_node_receipt(&receipt_path).map_err(|error| {
            GeoRunError::new(
                GeoRunErrorCode::ProjectRunFailed,
                "Geo run could not read a prior project node receipt",
                [
                    ("project_node_id", node.node_id.as_str()),
                    ("error", error.message.as_str()),
                ],
            )
        })?;
        candidate_receipts.insert(node.node_id.clone(), receipt);
    }

    let mut valid_receipts = BTreeMap::new();
    loop {
        let mut advanced = false;
        for node in &plan.nodes {
            if valid_receipts.contains_key(&node.node_id) {
                continue;
            }
            let Some(receipt) = candidate_receipts.get(&node.node_id) else {
                continue;
            };
            if !completed_receipt_matches_effective_node(plan, node, receipt, &valid_receipts) {
                continue;
            }
            let outputs = read_validated_receipt_outputs(policy, node, receipt)?;
            for output in outputs {
                executor
                    .insert_dependency_output(output)
                    .map_err(project_run_error)?;
            }
            valid_receipts.insert(node.node_id.clone(), receipt.clone());
            advanced = true;
        }
        if !advanced {
            break;
        }
    }
    Ok(valid_receipts)
}

fn completed_receipt_matches_effective_node(
    plan: &ProjectPlan,
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
) -> bool {
    let Some(dependency_semantic_hashes) =
        dependency_semantic_hashes_from_receipts(node, valid_receipts)
    else {
        return false;
    };
    receipt.project_id == plan.project_id
        && receipt.node_id == node.node_id
        && receipt.node_cache_key == node.cache.cache_key
        && receipt.outcome == ProjectRunNodeOutcome::Completed
        && receipt.content_hash_inputs == receipt_hash_inputs(&node.content_hash_inputs)
        && receipt.dependency_semantic_hashes == dependency_semantic_hashes
        && receipt_outputs_match_effective_node(node, receipt)
}

fn read_validated_receipt_outputs(
    policy: &ProjectRunPolicy,
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
) -> GeoRunResult<Vec<GeoExecutorDependencyOutput>> {
    validate_receipt_outputs_match_effective_node(node, receipt)?;
    let mut outputs = Vec::new();
    for output in &receipt.outputs {
        let contract = output_contract_for_output_id(&output.output_id).ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo run cannot infer a typed contract for a prior project output",
                [
                    ("project_node_id", receipt.node_id.as_str()),
                    ("output_id", output.output_id.as_str()),
                ],
            )
        })?;
        let bytes = read_receipt_output_bytes(policy, receipt, output)?;
        let content_hash = output.content_digest.clone();
        outputs.push(GeoExecutorDependencyOutput {
            producer_node_id: receipt.node_id.clone(),
            output_id: output.output_id.clone(),
            contract: contract.to_string(),
            content_hash,
            bytes,
        });
    }
    outputs.sort_by(|left, right| left.output_id.cmp(&right.output_id));
    Ok(outputs)
}

fn receipt_outputs_match_effective_node(
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
) -> bool {
    receipt.outputs.len() == node.outputs.len()
        && node.outputs.iter().all(|planned| {
            receipt
                .outputs
                .iter()
                .any(|actual| actual.output_id == planned.output_id && actual.path == planned.path)
        })
}

fn validate_receipt_outputs_match_effective_node(
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
) -> GeoRunResult<()> {
    if receipt_outputs_match_effective_node(node, receipt) {
        return Ok(());
    }
    Err(GeoRunError::new(
        GeoRunErrorCode::ArtifactContract,
        "Geo run receipt outputs do not match the current node's declared output ownership",
        [("project_node_id", node.node_id.as_str())],
    ))
}

fn read_receipt_output_bytes(
    policy: &ProjectRunPolicy,
    receipt: &ProjectRunNodeReceipt,
    output: &ProjectRunOutputReceipt,
) -> GeoRunResult<Vec<u8>> {
    let path = resolve_workspace_path(
        &policy.workspace_root,
        "geo_run.prior_project_output",
        Path::new(&output.path),
        PlannedAccess::Read,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        GeoRunError::new(
            GeoRunErrorCode::ProjectRunFailed,
            "Geo run prior project output path failed workspace safety",
            [
                ("project_node_id", receipt.node_id.clone()),
                ("output_id", output.output_id.clone()),
                ("error", error.to_string()),
            ],
        )
    })?;
    let bytes = fs::read(&path).map_err(|error| {
        GeoRunError::new(
            GeoRunErrorCode::ProjectRunFailed,
            "Geo run could not read a prior project output",
            [
                ("project_node_id", receipt.node_id.clone()),
                ("output_id", output.output_id.clone()),
                ("error", error.to_string()),
            ],
        )
    })?;
    let byte_count = bytes.len() as u64;
    let content_hash = digest_bytes(&bytes);
    if byte_count != output.byte_count || content_hash != output.content_digest {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ProjectRunFailed,
            "Geo run prior project output bytes no longer match their receipt",
            [
                ("project_node_id", receipt.node_id.as_str()),
                ("output_id", output.output_id.as_str()),
            ],
        ));
    }
    Ok(bytes)
}

fn final_composition_status_from_project_report(
    plan: &ProjectPlan,
    report: &ProjectRunReport,
    policy: &ProjectRunPolicy,
) -> GeoRunResult<Option<GeoCompositionStatus>> {
    let nodes = plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut statuses = Vec::new();
    for receipt in &report.receipt.node_receipts {
        if receipt.outcome != ProjectRunNodeOutcome::Completed {
            continue;
        }
        let node = nodes.get(receipt.node_id.as_str()).ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "Geo run report contains a completed receipt for an undeclared node",
                [("project_node_id", receipt.node_id.as_str())],
            )
        })?;
        validate_receipt_outputs_match_effective_node(node, receipt)?;
        if node.command != GEO_SOLVE_COMMAND {
            continue;
        }
        for output in &receipt.outputs {
            let bytes = read_receipt_output_bytes(policy, receipt, output)?;
            let artifact: GeoCompositionArtifact =
                serde_json::from_slice(&bytes).map_err(|error| {
                    GeoRunError::new(
                        GeoRunErrorCode::ArtifactContract,
                        "Geo run could not parse final composition output",
                        [
                            ("project_node_id", receipt.node_id.clone()),
                            ("output_id", output.output_id.clone()),
                            ("error", error.to_string()),
                        ],
                    )
                })?;
            if artifact.version != CANON_GEO_COMPOSITION_VERSION {
                return Err(GeoRunError::new(
                    GeoRunErrorCode::ArtifactContract,
                    "Geo run final composition output declares the wrong version",
                    [
                        ("project_node_id", receipt.node_id.as_str()),
                        ("output_id", output.output_id.as_str()),
                    ],
                ));
            }
            statuses.push(artifact.status);
        }
    }
    let status = coalesce_composition_statuses(statuses);
    let completed_solve = plan.nodes.iter().any(|node| {
        node.command == GEO_SOLVE_COMMAND && report.receipt.completed_nodes.contains(&node.node_id)
    });
    if completed_solve && status.is_none() {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run completed a solve node without a typed composition output",
            BTreeMap::<String, String>::new(),
        ));
    }
    Ok(status)
}

fn coalesce_composition_statuses(
    statuses: Vec<GeoCompositionStatus>,
) -> Option<GeoCompositionStatus> {
    if statuses.is_empty() {
        return None;
    }
    if statuses.contains(&GeoCompositionStatus::BudgetFallback) {
        return Some(GeoCompositionStatus::BudgetFallback);
    }
    if statuses.contains(&GeoCompositionStatus::Conflict) {
        return Some(GeoCompositionStatus::Conflict);
    }
    if statuses.contains(&GeoCompositionStatus::Ambiguous) {
        return Some(GeoCompositionStatus::Ambiguous);
    }
    Some(GeoCompositionStatus::Resolved)
}

fn receipt_hash_inputs(inputs: &[ProjectPlanHashRef]) -> Vec<ProjectRunHashRef> {
    let mut refs = inputs
        .iter()
        .map(|input| ProjectRunHashRef {
            ref_id: input.ref_id.clone(),
            content_hash: input.content_hash.clone(),
        })
        .collect::<Vec<_>>();
    refs.sort_by(|left, right| left.ref_id.cmp(&right.ref_id));
    refs
}

fn dependency_semantic_hashes_from_receipts(
    node: &ProjectPlanNode,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
) -> Option<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for dependency in &node.dependencies {
        hashes.insert(
            dependency.clone(),
            valid_receipts.get(dependency)?.semantic_hash.clone(),
        );
    }
    Some(hashes)
}

fn project_receipt_path(policy: &ProjectRunPolicy, node_id: &str) -> GeoRunResult<PathBuf> {
    let work_dir = normalize_relative_path(&policy.work_dir).map_err(|message| {
        GeoRunError::new(
            GeoRunErrorCode::InvalidInput,
            "Geo run work_dir is not workspace relative",
            [("error", message)],
        )
    })?;
    let relative = work_dir
        .join("receipts")
        .join(format!("{}.json", node_id_token(node_id)));
    resolve_workspace_path(
        &policy.workspace_root,
        "geo_run.project_receipt",
        &relative,
        PlannedAccess::Read,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        GeoRunError::new(
            GeoRunErrorCode::InvalidInput,
            "Geo run project receipt path failed workspace safety",
            [
                ("project_node_id", node_id.to_string()),
                ("error", error.to_string()),
            ],
        )
    })
}

fn normalize_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "path must stay relative to the project workspace: {}",
                    path.display()
                ));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err("path must contain at least one relative segment".to_string());
    }
    Ok(normalized)
}

fn node_id_token(node_id: &str) -> String {
    node_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn artifact_refs_from_bindings(
    bindings: &BTreeMap<(String, String), GeoRunArtifactBinding>,
) -> Vec<GeoRunArtifactRef> {
    bindings
        .values()
        .map(|binding| GeoRunArtifactRef {
            node_id: binding.node_id.clone(),
            binding_id: binding.binding_id.clone(),
            artifact_id: binding.artifact_id.clone(),
            content_digest: binding.content_digest.clone(),
            media_type: binding.media_type.clone(),
            contract_version: binding.contract_version.clone(),
            byte_count: binding.byte_count,
        })
        .collect()
}

fn output_refs_from_project_report(
    plan: &GeoPlan,
    report: &ProjectRunReport,
) -> GeoRunResult<Vec<GeoRunOutputRef>> {
    let nodes = plan
        .project_plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let contracts = plan
        .geo_nodes
        .iter()
        .map(|overlay| {
            (
                overlay.project_node_id.as_str(),
                overlay.expected_output_contract.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut refs = Vec::new();
    for receipt in &report.receipt.node_receipts {
        if receipt.outcome != ProjectRunNodeOutcome::Completed {
            continue;
        }
        let Some(contract) = contracts.get(receipt.node_id.as_str()) else {
            return Err(GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "project receipt node has no Geo contract overlay",
                [("project_node_id", receipt.node_id.as_str())],
            ));
        };
        let node = nodes.get(receipt.node_id.as_str()).ok_or_else(|| {
            GeoRunError::new(
                GeoRunErrorCode::ArtifactContract,
                "project receipt node is absent from the effective Geo project plan",
                [("project_node_id", receipt.node_id.as_str())],
            )
        })?;
        validate_receipt_outputs_match_effective_node(node, receipt)?;
        for output in &receipt.outputs {
            refs.push(output_ref(&receipt.node_id, output, contract));
        }
    }
    refs.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    Ok(refs)
}

fn output_ref(
    project_node_id: &str,
    output: &ProjectRunOutputReceipt,
    contract_version: &str,
) -> GeoRunOutputRef {
    GeoRunOutputRef {
        artifact_id: geo_run_declared_artifact_id(project_node_id, &output.output_id),
        project_node_id: project_node_id.to_string(),
        output_id: output.output_id.clone(),
        content_digest: output.content_digest.clone(),
        byte_count: output.byte_count,
        media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
        contract_version: contract_version.to_string(),
    }
}

fn grain_states(plan: &GeoPlan) -> Vec<GeoRunGrainState> {
    let mut states = plan
        .grain_outcomes
        .iter()
        .map(|outcome| GeoRunGrainState {
            entity_level: format!("{:?}", outcome.entity_level).to_lowercase(),
            status: outcome.status,
            missing_evidence_classes: outcome
                .missing_evidence_classes
                .iter()
                .map(|class| format!("{class:?}").to_lowercase())
                .collect(),
            project_node_ids: outcome.project_node_ids.clone(),
            claim_limitation: outcome.claim_limitation.clone(),
            next_action: outcome.next_action.clone(),
        })
        .collect::<Vec<_>>();
    states.sort_by(|left, right| left.entity_level.cmp(&right.entity_level));
    states
}

fn blockers_from_plan(plan: &GeoPlan) -> Vec<GeoRunBlocker> {
    let mut blockers = Vec::new();
    for outcome in &plan.grain_outcomes {
        match outcome.status {
            GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse => {}
            GeoPlanGrainStatus::WaitingForAcquisition => blockers.push(GeoRunBlocker {
                blocker_id: format!(
                    "waiting_for_input:{}",
                    format!("{:?}", outcome.entity_level).to_lowercase()
                ),
                kind: GeoRunBlockerKind::WaitingForInput,
                project_node_id: None,
                entity_level: Some(format!("{:?}", outcome.entity_level).to_lowercase()),
                reason: outcome.claim_limitation.clone(),
            }),
            GeoPlanGrainStatus::MissingLeafCapability => blockers.push(GeoRunBlocker {
                blocker_id: format!(
                    "missing_leaf:{}",
                    format!("{:?}", outcome.entity_level).to_lowercase()
                ),
                kind: GeoRunBlockerKind::MissingLeafCapability,
                project_node_id: None,
                entity_level: Some(format!("{:?}", outcome.entity_level).to_lowercase()),
                reason: outcome.next_action.clone(),
            }),
            GeoPlanGrainStatus::UnsupportedByProfile
            | GeoPlanGrainStatus::UnsupportedByInventory => {
                blockers.push(GeoRunBlocker {
                    blocker_id: format!(
                        "unsupported:{}",
                        format!("{:?}", outcome.entity_level).to_lowercase()
                    ),
                    kind: GeoRunBlockerKind::UnsupportedGrain,
                    project_node_id: None,
                    entity_level: Some(format!("{:?}", outcome.entity_level).to_lowercase()),
                    reason: outcome.claim_limitation.clone(),
                });
            }
        }
    }
    blockers
}

fn blockers_from_project_report(report: Option<&ProjectRunReport>) -> Vec<GeoRunBlocker> {
    let Some(report) = report else {
        return Vec::new();
    };
    let mut blockers = Vec::new();
    for node_id in &report.failed_nodes {
        let reason = report
            .node_reports
            .iter()
            .find(|node| node.node_id == *node_id)
            .and_then(|node| node.reason.clone())
            .unwrap_or_else(|| "project node failed".to_string());
        blockers.push(GeoRunBlocker {
            blocker_id: format!("failed:{node_id}"),
            kind: GeoRunBlockerKind::ProjectFailure,
            project_node_id: Some(node_id.clone()),
            entity_level: None,
            reason,
        });
    }
    for node_id in &report.cancelled_nodes {
        blockers.push(GeoRunBlocker {
            blocker_id: format!("cancelled:{node_id}"),
            kind: GeoRunBlockerKind::ProjectCancelled,
            project_node_id: Some(node_id.clone()),
            entity_level: None,
            reason: "project run cancelled before this node completed".to_string(),
        });
    }
    for node_id in &report.blocked_nodes {
        blockers.push(GeoRunBlocker {
            blocker_id: format!("blocked:{node_id}"),
            kind: GeoRunBlockerKind::ProjectBlocked,
            project_node_id: Some(node_id.clone()),
            entity_level: None,
            reason: "project node is waiting for a declared dependency".to_string(),
        });
    }
    blockers
}

fn next_actions_from_plan(plan: &GeoPlan) -> Vec<GeoRunNextAction> {
    let mut actions = Vec::new();
    for external in &plan.external_requests {
        match external {
            GeoPlanExternalRequest::Acquisition { request, handoff } => {
                actions.push(GeoRunNextAction {
                    action_id: request.request_id.clone(),
                    kind: GeoRunNextActionKind::SatisfyAcquisition,
                    project_node_id: None,
                    artifact_id: Some(request.request_id.clone()),
                    expected_contract: Some(handoff.expected_receipt_contract.clone()),
                    media_type: Some(GEO_RUN_JSON_MEDIA_TYPE.to_string()),
                    command: Some(handoff.continuation_command.clone()),
                    reason: "satisfy the typed acquisition request and re-plan with the verified local artifact".to_string(),
                });
            }
            GeoPlanExternalRequest::Discovery { gap_id, request } => {
                actions.push(GeoRunNextAction {
                    action_id: request.request_id.clone(),
                    kind: GeoRunNextActionKind::SatisfyDiscovery,
                    project_node_id: None,
                    artifact_id: Some(gap_id.clone()),
                    expected_contract: None,
                    media_type: Some(GEO_RUN_JSON_MEDIA_TYPE.to_string()),
                    command: None,
                    reason: "route the typed discovery request to an external executor; Canon does not reach the network from the run".to_string(),
                });
            }
            GeoPlanExternalRequest::DiscoveryGap { gap } => {
                actions.push(GeoRunNextAction {
                    action_id: gap.gap_id.clone(),
                    kind: GeoRunNextActionKind::SatisfyDiscovery,
                    project_node_id: None,
                    artifact_id: Some(gap.gap_id.clone()),
                    expected_contract: None,
                    media_type: Some(GEO_RUN_JSON_MEDIA_TYPE.to_string()),
                    command: Some(gap.next_command.clone()),
                    reason: gap.reason.clone(),
                });
            }
        }
    }
    for outcome in &plan.grain_outcomes {
        if outcome.status == GeoPlanGrainStatus::UnsupportedByProfile
            || outcome.status == GeoPlanGrainStatus::UnsupportedByInventory
            || outcome.status == GeoPlanGrainStatus::MissingLeafCapability
        {
            actions.push(GeoRunNextAction {
                action_id: format!(
                    "unsupported:{}",
                    format!("{:?}", outcome.entity_level).to_lowercase()
                ),
                kind: GeoRunNextActionKind::UnsupportedGrain,
                project_node_id: None,
                artifact_id: None,
                expected_contract: None,
                media_type: None,
                command: None,
                reason: outcome.next_action.clone(),
            });
        }
    }
    actions
}

fn next_actions_from_project_report(report: Option<&ProjectRunReport>) -> Vec<GeoRunNextAction> {
    let Some(report) = report else {
        return Vec::new();
    };
    let mut actions = report
        .next_actions
        .iter()
        .map(|(node_id, command)| GeoRunNextAction {
            action_id: format!("execute:{node_id}"),
            kind: GeoRunNextActionKind::ExecuteProjectNode,
            project_node_id: Some(node_id.clone()),
            artifact_id: None,
            expected_contract: None,
            media_type: None,
            command: Some(command.clone()),
            reason: "project node is ready under the shared run receipt state".to_string(),
        })
        .collect::<Vec<_>>();
    for node_id in &report.failed_nodes {
        actions.push(GeoRunNextAction {
            action_id: format!("inspect:{node_id}"),
            kind: GeoRunNextActionKind::InspectFailure,
            project_node_id: Some(node_id.clone()),
            artifact_id: None,
            expected_contract: None,
            media_type: None,
            command: None,
            reason: "inspect the failed project node receipt before retrying".to_string(),
        });
    }
    for node_id in &report.cancelled_nodes {
        actions.push(GeoRunNextAction {
            action_id: format!("resume:{node_id}"),
            kind: GeoRunNextActionKind::Resume,
            project_node_id: Some(node_id.clone()),
            artifact_id: None,
            expected_contract: None,
            media_type: None,
            command: None,
            reason: "resume the cancelled project run from the last valid receipt".to_string(),
        });
    }
    actions
}

fn run_status(
    plan: &GeoPlan,
    report: Option<&ProjectRunReport>,
    composition_status: Option<GeoCompositionStatus>,
    blockers: &[GeoRunBlocker],
    next_actions: &[GeoRunNextAction],
) -> GeoRunStatus {
    if report.is_some_and(|report| !report.failed_nodes.is_empty()) {
        return GeoRunStatus::Failed;
    }
    if report.is_some_and(|report| !report.cancelled_nodes.is_empty()) {
        return GeoRunStatus::Cancelled;
    }
    match composition_status {
        Some(GeoCompositionStatus::BudgetFallback) => return GeoRunStatus::BudgetFallback,
        Some(GeoCompositionStatus::Conflict) => return GeoRunStatus::Contradicted,
        Some(GeoCompositionStatus::Ambiguous) => return GeoRunStatus::Abstained,
        Some(GeoCompositionStatus::Resolved) | None => {}
    }
    if blockers
        .iter()
        .any(|blocker| blocker.kind == GeoRunBlockerKind::WaitingForInput)
        || next_actions.iter().any(|action| {
            matches!(
                action.kind,
                GeoRunNextActionKind::SatisfyAcquisition
                    | GeoRunNextActionKind::SatisfyDiscovery
                    | GeoRunNextActionKind::SupplyLocalArtifact
            )
        })
    {
        return GeoRunStatus::WaitingForInput;
    }
    if plan.status == GeoPlanStatus::Unsupported
        || blockers
            .iter()
            .all(|blocker| blocker.kind == GeoRunBlockerKind::UnsupportedGrain)
            && !blockers.is_empty()
            && plan.project_plan.nodes.is_empty()
    {
        return GeoRunStatus::UnsupportedGrain;
    }
    if blockers.iter().any(|blocker| {
        matches!(
            blocker.kind,
            GeoRunBlockerKind::UnsupportedGrain | GeoRunBlockerKind::MissingLeafCapability
        )
    }) {
        return GeoRunStatus::Partial;
    }
    if report.is_some_and(|report| {
        report.receipt.completed_nodes.len() == plan.project_plan.nodes.len()
            && report.blocked_nodes.is_empty()
            && report.next_actions.is_empty()
    }) {
        return GeoRunStatus::Completed;
    }
    GeoRunStatus::Partial
}

fn run_phase(
    plan: &GeoPlan,
    report: Option<&ProjectRunReport>,
    status: GeoRunStatus,
) -> GeoRunPhase {
    if matches!(
        status,
        GeoRunStatus::WaitingForInput | GeoRunStatus::UnsupportedGrain
    ) && report.is_none()
    {
        return GeoRunPhase::Preflighted;
    }
    let Some(report) = report else {
        return GeoRunPhase::Drafted;
    };
    let completed = report
        .receipt
        .completed_nodes
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut phase = GeoRunPhase::Preflighted;
    for overlay in &plan.geo_nodes {
        if completed.contains(&overlay.project_node_id) {
            phase = phase.max(phase_for_stage(overlay.stage));
        }
    }
    phase
}

fn phase_for_stage(stage: GeoPlanStage) -> GeoRunPhase {
    match stage {
        GeoPlanStage::MaterializeHomeCells | GeoPlanStage::MaterializeEvidence => {
            GeoRunPhase::Materialized
        }
        GeoPlanStage::BuildBoundedSection => GeoRunPhase::ReachChecked,
        GeoPlanStage::CompileEvidence => GeoRunPhase::Compiled,
        GeoPlanStage::FactorAndSolveExactResidual => GeoRunPhase::Solved,
    }
}

fn project_node_ids_in_dependency_order(plan: &ProjectPlan) -> Vec<String> {
    let mut remaining = plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut admitted = BTreeSet::new();
    let mut ordered = Vec::with_capacity(plan.nodes.len());
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .filter(|(_, node)| {
                node.dependencies
                    .iter()
                    .all(|dependency| admitted.contains(dependency))
            })
            .map(|(node_id, _)| *node_id)
            .collect::<Vec<_>>();
        if ready.is_empty() {
            break;
        }
        for node_id in ready {
            remaining.remove(node_id);
            admitted.insert(node_id.to_string());
            ordered.push(node_id.to_string());
        }
    }
    ordered
}

fn project_target_node_ids(
    plan: &ProjectPlan,
    selected_nodes: &BTreeSet<String>,
) -> Option<BTreeSet<String>> {
    if selected_nodes.is_empty() {
        return Some(plan.nodes.iter().map(|node| node.node_id.clone()).collect());
    }
    let nodes = plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut targets = BTreeSet::new();
    let mut pending = selected_nodes.iter().cloned().collect::<Vec<_>>();
    while let Some(node_id) = pending.pop() {
        let node = nodes.get(node_id.as_str())?;
        if !targets.insert(node_id) {
            continue;
        }
        pending.extend(node.dependencies.iter().cloned());
    }
    Some(targets)
}

fn aggregate_deterministic_usage(report: Option<&ProjectRunReport>) -> BTreeMap<String, u64> {
    let mut usage = BTreeMap::new();
    let Some(report) = report else {
        return usage;
    };
    for receipt in &report.receipt.node_receipts {
        for (key, value) in &receipt.deterministic_usage {
            *usage.entry(key.clone()).or_insert(0) += value;
        }
    }
    usage
}

fn sort_and_dedup_run_parts(
    grain_states: &mut [GeoRunGrainState],
    blockers: &mut Vec<GeoRunBlocker>,
    next_actions: &mut Vec<GeoRunNextAction>,
    artifact_inputs: &mut [GeoRunArtifactRef],
) {
    for state in grain_states.iter_mut() {
        state.missing_evidence_classes.sort();
        state.missing_evidence_classes.dedup();
        state.project_node_ids.sort();
        state.project_node_ids.dedup();
    }
    grain_states.sort_by(|left, right| left.entity_level.cmp(&right.entity_level));
    blockers.sort_by(|left, right| left.blocker_id.cmp(&right.blocker_id));
    blockers.dedup_by(|left, right| left.blocker_id == right.blocker_id);
    next_actions.sort_by(|left, right| left.action_id.cmp(&right.action_id));
    next_actions.dedup_by(|left, right| left.action_id == right.action_id);
    artifact_inputs.sort();
}

#[derive(Serialize)]
struct GeoRunSemanticProjection<'a> {
    version: &'a str,
    status: GeoRunStatus,
    phase: GeoRunPhase,
    plan_ref: &'a GeoRunPlanRef,
    artifact_inputs: &'a [GeoRunArtifactRef],
    output_refs: &'a [GeoRunOutputRef],
    grain_states: Vec<GeoRunGrainSemanticProjection<'a>>,
    blockers: Vec<GeoRunBlockerSemanticProjection<'a>>,
    next_actions: Vec<GeoRunNextActionSemanticProjection<'a>>,
    deterministic_usage: &'a BTreeMap<String, u64>,
    project_run: Option<GeoProjectRunSemanticProjection<'a>>,
}

#[derive(Serialize)]
struct GeoRunGrainSemanticProjection<'a> {
    entity_level: &'a str,
    status: GeoPlanGrainStatus,
    missing_evidence_classes: &'a [String],
    project_node_ids: &'a [String],
}

#[derive(Serialize)]
struct GeoRunBlockerSemanticProjection<'a> {
    blocker_id: &'a str,
    kind: GeoRunBlockerKind,
    project_node_id: &'a Option<String>,
    entity_level: &'a Option<String>,
}

#[derive(Serialize)]
struct GeoRunNextActionSemanticProjection<'a> {
    action_id: &'a str,
    kind: GeoRunNextActionKind,
    project_node_id: &'a Option<String>,
    artifact_id: &'a Option<String>,
    expected_contract: &'a Option<String>,
    media_type: &'a Option<String>,
    command: &'a Option<String>,
}

#[derive(Serialize)]
struct GeoProjectRunSemanticProjection<'a> {
    schema_version: &'a str,
    project_id: &'a str,
    plan_graph_hash: &'a str,
    completed_nodes: &'a [String],
    failed_nodes: &'a [String],
    cancelled_nodes: &'a [String],
    invalidated_nodes: &'a [String],
    blocked_nodes: &'a [String],
    nodes: Vec<GeoProjectNodeSemanticProjection<'a>>,
}

#[derive(Serialize)]
struct GeoProjectNodeSemanticProjection<'a> {
    node_id: &'a str,
    outcome: ProjectRunNodeOutcome,
    content_hash_inputs: &'a [ProjectRunHashRef],
    dependency_semantic_hashes: &'a BTreeMap<String, String>,
    outputs: Vec<GeoProjectOutputSemanticProjection<'a>>,
    deterministic_usage: &'a BTreeMap<String, u64>,
    next_action: ProjectRunNextAction,
    failure_code: &'a Option<String>,
}

#[derive(Serialize)]
struct GeoProjectOutputSemanticProjection<'a> {
    output_id: &'a str,
    content_digest: &'a str,
    byte_count: u64,
}

fn semantic_projection(run: &GeoRun) -> GeoRunSemanticProjection<'_> {
    let grain_states = run
        .grain_states
        .iter()
        .map(|grain| GeoRunGrainSemanticProjection {
            entity_level: &grain.entity_level,
            status: grain.status,
            missing_evidence_classes: &grain.missing_evidence_classes,
            project_node_ids: &grain.project_node_ids,
        })
        .collect();
    let blockers = run
        .blockers
        .iter()
        .map(|blocker| GeoRunBlockerSemanticProjection {
            blocker_id: &blocker.blocker_id,
            kind: blocker.kind,
            project_node_id: &blocker.project_node_id,
            entity_level: &blocker.entity_level,
        })
        .collect();
    let next_actions = run
        .next_actions
        .iter()
        .map(|action| GeoRunNextActionSemanticProjection {
            action_id: &action.action_id,
            kind: action.kind,
            project_node_id: &action.project_node_id,
            artifact_id: &action.artifact_id,
            expected_contract: &action.expected_contract,
            media_type: &action.media_type,
            command: &action.command,
        })
        .collect();
    GeoRunSemanticProjection {
        version: &run.version,
        status: run.status,
        phase: run.phase,
        plan_ref: &run.plan_ref,
        artifact_inputs: &run.artifact_inputs,
        output_refs: &run.output_refs,
        grain_states,
        blockers,
        next_actions,
        deterministic_usage: &run.deterministic_usage,
        project_run: run
            .project_run_report
            .as_ref()
            .map(project_run_semantic_projection),
    }
}

fn project_run_semantic_projection(
    report: &ProjectRunReport,
) -> GeoProjectRunSemanticProjection<'_> {
    GeoProjectRunSemanticProjection {
        schema_version: &report.schema_version,
        project_id: &report.project_id,
        plan_graph_hash: &report.plan_graph_hash,
        completed_nodes: &report.receipt.completed_nodes,
        failed_nodes: &report.receipt.failed_nodes,
        cancelled_nodes: &report.receipt.cancelled_nodes,
        invalidated_nodes: &report.receipt.invalidated_nodes,
        blocked_nodes: &report.receipt.blocked_nodes,
        nodes: report
            .receipt
            .node_receipts
            .iter()
            .map(|receipt| GeoProjectNodeSemanticProjection {
                node_id: &receipt.node_id,
                outcome: receipt.outcome,
                content_hash_inputs: &receipt.content_hash_inputs,
                dependency_semantic_hashes: &receipt.dependency_semantic_hashes,
                outputs: receipt
                    .outputs
                    .iter()
                    .map(|output| GeoProjectOutputSemanticProjection {
                        output_id: &output.output_id,
                        content_digest: &output.content_digest,
                        byte_count: output.byte_count,
                    })
                    .collect(),
                deterministic_usage: &receipt.deterministic_usage,
                next_action: receipt.next_action,
                failure_code: &receipt.failure_code,
            })
            .collect(),
    }
}

fn validate_artifact_ref(input: &GeoRunArtifactRef) -> GeoRunResult<()> {
    if input.node_id.trim().is_empty()
        || input.binding_id.trim().is_empty()
        || input.artifact_id.trim().is_empty()
        || input.media_type.trim().is_empty()
        || input.contract_version.trim().is_empty()
    {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run artifact refs require artifact id, media type, and contract version",
            [("artifact_id", input.artifact_id.as_str())],
        ));
    }
    validate_project_node_id("artifact_ref.node_id", &input.node_id)?;
    validate_local_id("artifact_ref.binding_id", &input.binding_id, 256)?;
    let expected = geo_run_input_artifact_id(&input.node_id, &input.binding_id);
    if input.artifact_id != expected {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run input artifact ref id must be derived from project node and binding id",
            [
                ("expected", expected),
                ("actual", input.artifact_id.clone()),
            ],
        ));
    }
    validate_json_contract(
        "artifact_ref.media_type",
        "artifact_ref.contract_version",
        &input.media_type,
        &input.contract_version,
    )?;
    validate_digest("artifact_ref.content_digest", &input.content_digest)
}

fn validate_json_contract(
    media_field: &str,
    contract_field: &str,
    media_type: &str,
    contract_version: &str,
) -> GeoRunResult<()> {
    if media_type != GEO_RUN_JSON_MEDIA_TYPE {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run artifact refs currently require JSON media type",
            [("field", media_field), ("media_type", media_type)],
        ));
    }
    if !is_contract_version(contract_version) {
        return Err(GeoRunError::new(
            GeoRunErrorCode::ArtifactContract,
            "Geo run contract versions must be canonical versioned Canon identifiers",
            [("field", contract_field), ("value", contract_version)],
        ));
    }
    Ok(())
}

fn validate_project_run_report_schema_shape(report: &ProjectRunReport) -> GeoRunResult<()> {
    if report.schema_version != CANON_PROJECT_RUN_VERSION {
        return Err(project_report_contract_error(
            "project_run_report.schema_version",
            &report.schema_version,
        ));
    }
    validate_project_non_empty_text("project_run_report.project_id", &report.project_id)?;
    validate_project_digest(
        "project_run_report.plan_graph_hash",
        &report.plan_graph_hash,
    )?;
    validate_project_digest(
        "project_run_report.run_receipt_hash",
        &report.run_receipt_hash,
    )?;
    validate_portable_usize(
        "project_run_report.max_parallelism",
        report.max_parallelism,
        true,
    )?;
    validate_portable_usize(
        "project_run_report.max_ready_width",
        report.max_ready_width,
        false,
    )?;
    validate_project_unique_strings("project_run_report.executed_nodes", &report.executed_nodes)?;
    validate_project_unique_strings("project_run_report.resumed_nodes", &report.resumed_nodes)?;
    validate_project_unique_strings("project_run_report.failed_nodes", &report.failed_nodes)?;
    validate_project_unique_strings(
        "project_run_report.cancelled_nodes",
        &report.cancelled_nodes,
    )?;
    validate_project_unique_strings(
        "project_run_report.invalidated_nodes",
        &report.invalidated_nodes,
    )?;
    validate_project_unique_strings("project_run_report.blocked_nodes", &report.blocked_nodes)?;
    for command in report.next_actions.values() {
        validate_project_non_empty_text("project_run_report.next_actions", command)?;
    }
    validate_project_run_receipt_schema_shape("project_run_report.receipt", &report.receipt)?;
    for node_report in &report.node_reports {
        validate_project_non_empty_text(
            "project_run_report.node_reports.node_id",
            &node_report.node_id,
        )?;
        if let Some(receipt_hash) = &node_report.receipt_hash {
            validate_project_digest("project_run_report.node_reports.receipt_hash", receipt_hash)?;
        }
        if let Some(reason) = &node_report.reason {
            validate_project_non_empty_text("project_run_report.node_reports.reason", reason)?;
        }
    }
    Ok(())
}

fn validate_project_run_receipt_schema_shape(
    field: &str,
    receipt: &crate::project::ProjectRunReceipt,
) -> GeoRunResult<()> {
    if receipt.schema_version != CANON_PROJECT_RUN_VERSION {
        return Err(project_report_contract_error(
            &format!("{field}.schema_version"),
            &receipt.schema_version,
        ));
    }
    validate_project_non_empty_text(&format!("{field}.project_id"), &receipt.project_id)?;
    validate_project_digest(
        &format!("{field}.plan_graph_hash"),
        &receipt.plan_graph_hash,
    )?;
    validate_project_digest(&format!("{field}.receipt_hash"), &receipt.receipt_hash)?;
    validate_project_unique_strings(
        &format!("{field}.completed_nodes"),
        &receipt.completed_nodes,
    )?;
    validate_project_unique_strings(&format!("{field}.failed_nodes"), &receipt.failed_nodes)?;
    validate_project_unique_strings(
        &format!("{field}.cancelled_nodes"),
        &receipt.cancelled_nodes,
    )?;
    validate_project_unique_strings(
        &format!("{field}.invalidated_nodes"),
        &receipt.invalidated_nodes,
    )?;
    validate_project_unique_strings(&format!("{field}.blocked_nodes"), &receipt.blocked_nodes)?;
    for node_receipt in &receipt.node_receipts {
        validate_project_node_receipt_schema_shape(
            &format!("{field}.node_receipts"),
            node_receipt,
        )?;
    }
    Ok(())
}

fn validate_project_node_receipt_schema_shape(
    field: &str,
    receipt: &ProjectRunNodeReceipt,
) -> GeoRunResult<()> {
    if receipt.schema_version != CANON_PROJECT_RUN_VERSION {
        return Err(project_report_contract_error(
            &format!("{field}.schema_version"),
            &receipt.schema_version,
        ));
    }
    validate_project_non_empty_text(&format!("{field}.project_id"), &receipt.project_id)?;
    validate_project_digest(
        &format!("{field}.plan_graph_hash"),
        &receipt.plan_graph_hash,
    )?;
    validate_project_non_empty_text(&format!("{field}.node_id"), &receipt.node_id)?;
    validate_project_digest(&format!("{field}.node_cache_key"), &receipt.node_cache_key)?;
    for input in &receipt.content_hash_inputs {
        validate_project_non_empty_text(
            &format!("{field}.content_hash_inputs.ref_id"),
            &input.ref_id,
        )?;
        validate_project_digest(
            &format!("{field}.content_hash_inputs.content_hash"),
            &input.content_hash,
        )?;
    }
    for digest in receipt.dependency_semantic_hashes.values() {
        validate_project_digest(&format!("{field}.dependency_semantic_hashes"), digest)?;
    }
    for digest in receipt.dependency_receipt_hashes.values() {
        validate_project_digest(&format!("{field}.dependency_receipt_hashes"), digest)?;
    }
    for output in &receipt.outputs {
        validate_project_non_empty_text(&format!("{field}.outputs.output_id"), &output.output_id)?;
        validate_project_non_empty_text(&format!("{field}.outputs.path"), &output.path)?;
        validate_project_digest(
            &format!("{field}.outputs.content_digest"),
            &output.content_digest,
        )?;
    }
    if let Some(failure_code) = &receipt.failure_code {
        validate_project_non_empty_text(&format!("{field}.failure_code"), failure_code)?;
    }
    if let Some(failure_message) = &receipt.failure_message {
        validate_project_non_empty_text(&format!("{field}.failure_message"), failure_message)?;
    }
    validate_project_digest(&format!("{field}.semantic_hash"), &receipt.semantic_hash)?;
    validate_project_digest(&format!("{field}.telemetry_hash"), &receipt.telemetry_hash)?;
    validate_project_digest(&format!("{field}.receipt_hash"), &receipt.receipt_hash)?;
    Ok(())
}

fn validate_portable_usize(field: &str, value: usize, require_positive: bool) -> GeoRunResult<()> {
    if require_positive && value == 0 {
        return Err(project_report_contract_error(field, &value.to_string()));
    }
    if value > u32::MAX as usize {
        return Err(project_report_contract_error(field, &value.to_string()));
    }
    Ok(())
}

fn validate_project_unique_strings(field: &str, values: &[String]) -> GeoRunResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        validate_project_non_empty_text(field, value)?;
        if !seen.insert(value) {
            return Err(project_report_contract_error(field, value));
        }
    }
    Ok(())
}

fn validate_project_non_empty_text(field: &str, value: &str) -> GeoRunResult<()> {
    if !value.is_empty() {
        return Ok(());
    }
    Err(project_report_contract_error(field, value))
}

fn validate_project_digest(field: &str, value: &str) -> GeoRunResult<()> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(project_report_contract_error(field, value));
    };
    if hex.len() == 64 && hex.bytes().all(is_lowercase_hex) {
        return Ok(());
    }
    Err(project_report_contract_error(field, value))
}

fn project_report_contract_error(field: &str, value: &str) -> GeoRunError {
    GeoRunError::new(
        GeoRunErrorCode::ArtifactContract,
        "Geo run embedded project_run_report does not match its schema vocabulary or bounds",
        [("field", field), ("value", value)],
    )
}

fn validate_run_state_invariants(run: &GeoRun) -> GeoRunResult<()> {
    let valid = match run.status {
        GeoRunStatus::Completed => {
            run.project_run_report.is_some()
                && run.blockers.is_empty()
                && run.next_actions.is_empty()
        }
        GeoRunStatus::WaitingForInput => !run.blockers.is_empty() || !run.next_actions.is_empty(),
        GeoRunStatus::Failed | GeoRunStatus::Cancelled => {
            run.project_run_report.is_some() && !run.blockers.is_empty()
        }
        _ => true,
    };
    if valid {
        return Ok(());
    }
    Err(GeoRunError::new(
        GeoRunErrorCode::ArtifactContract,
        "Geo run status is inconsistent with its report, blockers, or next actions",
        [("status", format!("{:?}", run.status))],
    ))
}

fn validate_run_state_shapes(run: &GeoRun) -> GeoRunResult<()> {
    for state in &run.grain_states {
        validate_entity_level("grain_state.entity_level", &state.entity_level)?;
        for evidence_class in &state.missing_evidence_classes {
            validate_non_empty_text("grain_state.missing_evidence_class", evidence_class)?;
        }
        for node_id in &state.project_node_ids {
            validate_project_node_id("grain_state.project_node_id", node_id)?;
        }
        validate_non_empty_text("grain_state.claim_limitation", &state.claim_limitation)?;
        validate_non_empty_text("grain_state.next_action", &state.next_action)?;
    }
    for blocker in &run.blockers {
        validate_non_empty_text("blocker.blocker_id", &blocker.blocker_id)?;
        if let Some(node_id) = &blocker.project_node_id {
            validate_project_node_id("blocker.project_node_id", node_id)?;
        }
        if let Some(entity_level) = &blocker.entity_level {
            validate_entity_level("blocker.entity_level", entity_level)?;
        }
        validate_non_empty_text("blocker.reason", &blocker.reason)?;
    }
    for action in &run.next_actions {
        validate_non_empty_text("next_action.action_id", &action.action_id)?;
        if let Some(node_id) = &action.project_node_id {
            validate_project_node_id("next_action.project_node_id", node_id)?;
        }
        if let Some(artifact_id) = &action.artifact_id {
            validate_non_empty_text("next_action.artifact_id", artifact_id)?;
        }
        if let Some(contract) = &action.expected_contract {
            validate_expected_contract("next_action.expected_contract", contract)?;
        }
        if let Some(media_type) = &action.media_type {
            validate_non_empty_text("next_action.media_type", media_type)?;
        }
        if let Some(command) = &action.command {
            validate_non_empty_text("next_action.command", command)?;
        }
        validate_non_empty_text("next_action.reason", &action.reason)?;
    }
    for (field, value) in [
        (
            "observation.workspace_path",
            run.observation.workspace_path.as_deref(),
        ),
        (
            "observation.observed_at_utc",
            run.observation.observed_at_utc.as_deref(),
        ),
        ("observation.host_id", run.observation.host_id.as_deref()),
    ] {
        if let Some(value) = value {
            validate_non_empty_text(field, value)?;
        }
    }
    Ok(())
}

fn validate_canonical_run_order(run: &GeoRun) -> GeoRunResult<()> {
    let ordered = strictly_increasing_by(&run.artifact_inputs, |item| {
        format!("{}\0{}", item.node_id, item.binding_id)
    }) && strictly_increasing_by(&run.output_refs, |item| item.artifact_id.clone())
        && strictly_increasing_by(&run.grain_states, |item| item.entity_level.clone())
        && strictly_increasing_by(&run.blockers, |item| item.blocker_id.clone())
        && strictly_increasing_by(&run.next_actions, |item| item.action_id.clone())
        && run.grain_states.iter().all(|state| {
            strictly_increasing(&state.missing_evidence_classes)
                && strictly_increasing(&state.project_node_ids)
        });
    if ordered {
        return Ok(());
    }
    Err(GeoRunError::new(
        GeoRunErrorCode::ArtifactContract,
        "Geo run repeated collections must be uniquely sorted in canonical order",
        BTreeMap::<String, String>::new(),
    ))
}

fn strictly_increasing(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn strictly_increasing_by<T>(values: &[T], key: impl Fn(&T) -> String) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn validate_plan_id(field: &str, value: &str) -> GeoRunResult<()> {
    let valid = value
        .strip_prefix("canon_geo_plan.v0:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(is_lowercase_hex));
    if valid {
        return Ok(());
    }
    Err(invalid_identifier(field, value))
}

fn validate_local_id(field: &str, value: &str, max_len: usize) -> GeoRunResult<()> {
    let mut bytes = value.bytes();
    let valid = value.len() <= max_len
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && bytes.all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'_' | b':' | b'/' | b'@' | b'+' | b'-')
        });
    if valid {
        return Ok(());
    }
    Err(invalid_identifier(field, value))
}

fn validate_project_node_id(field: &str, value: &str) -> GeoRunResult<()> {
    let mut bytes = value.bytes();
    let valid = value.len() <= 128
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b':' | b'-')
        });
    if valid {
        return Ok(());
    }
    Err(invalid_identifier(field, value))
}

fn is_contract_version(value: &str) -> bool {
    if value.len() > 128 {
        return false;
    }
    let Some(rest) = value
        .strip_prefix("canon_")
        .or_else(|| value.strip_prefix("canon."))
    else {
        return false;
    };
    let Some((name, version)) = rest.rsplit_once(".v") else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        && !version.is_empty()
        && version.bytes().all(|byte| byte.is_ascii_digit())
}

fn validate_expected_contract(field: &str, value: &str) -> GeoRunResult<()> {
    if value.len() <= 512 && value.split('|').all(is_contract_version) {
        return Ok(());
    }
    Err(invalid_identifier(field, value))
}

fn validate_non_empty_text(field: &str, value: &str) -> GeoRunResult<()> {
    if !value.is_empty() && value.len() <= 4096 {
        return Ok(());
    }
    Err(invalid_identifier(field, value))
}

fn validate_entity_level(field: &str, value: &str) -> GeoRunResult<()> {
    if matches!(
        value,
        "site" | "property" | "parcel" | "building" | "unit" | "address" | "poi"
    ) {
        return Ok(());
    }
    Err(invalid_identifier(field, value))
}

fn invalid_identifier(field: &str, value: &str) -> GeoRunError {
    GeoRunError::new(
        GeoRunErrorCode::ArtifactContract,
        "Geo run identifier does not match its canonical contract",
        [("field", field), ("value", value)],
    )
}

fn validate_digest(field: &str, value: &str) -> GeoRunResult<()> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(invalid_digest(field, value));
    };
    if hex.len() != 64 || !hex.bytes().all(is_lowercase_hex) {
        return Err(invalid_digest(field, value));
    }
    Ok(())
}

fn is_lowercase_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn invalid_digest(field: &str, value: &str) -> GeoRunError {
    GeoRunError::new(
        GeoRunErrorCode::ArtifactContract,
        "Geo run digests must be canonical lowercase blake3 hex",
        [("field", field), ("value", value)],
    )
}

fn validate_run_policy(policy: &ProjectRunPolicy) -> GeoRunResult<()> {
    if policy.allow_network {
        return Err(GeoRunError::new(
            GeoRunErrorCode::InvalidInput,
            "Geo run refuses network-enabled project run policies",
            BTreeMap::<String, String>::new(),
        ));
    }
    if policy.allow_mutation_gates {
        return Err(GeoRunError::new(
            GeoRunErrorCode::InvalidInput,
            "Geo run refuses mutation gate execution; Geo remains review-gated workbench output",
            BTreeMap::<String, String>::new(),
        ));
    }
    if policy.failure_policy != ProjectRunFailurePolicy::FailFast {
        return Err(GeoRunError::new(
            GeoRunErrorCode::InvalidInput,
            "Geo run currently supports only fail-fast project execution",
            [("failure_policy", format!("{:?}", policy.failure_policy))],
        ));
    }
    Ok(())
}

fn plan_error(error: GeoPlanError) -> GeoRunError {
    GeoRunError::new(
        match error.code {
            crate::geo::GeoPlanErrorCode::UnsupportedVersion => GeoRunErrorCode::UnsupportedVersion,
            _ => GeoRunErrorCode::ArtifactContract,
        },
        format!("Geo plan validation failed: {}", error.message),
        error.detail,
    )
}

fn project_run_error(error: ProjectRunError) -> GeoRunError {
    GeoRunError::new(
        GeoRunErrorCode::ProjectRunFailed,
        format!("project run failed: {}", error.message),
        [
            ("project_run_error_code", format!("{:?}", error.code)),
            (
                "node_id",
                error.node_id.unwrap_or_else(|| "<none>".to_string()),
            ),
        ],
    )
}

fn serialization_error(error: serde_json::Error) -> GeoRunError {
    GeoRunError::new(
        GeoRunErrorCode::Serialization,
        format!("failed to serialize Geo run artifact: {error}"),
        BTreeMap::<String, String>::new(),
    )
}
