#![forbid(unsafe_code)]

use super::{
    lock::{ProjectLock, ProjectLockRefKind, project_lock_digest},
    manifest::{
        ProjectManifest, ProjectManifestError, ProjectModeKind, ProjectNetworkPolicy,
        ProjectOutputDeclaration, ProjectOutputKind, ProjectPackageBinding, ProjectPackageKind,
        ProjectResourceBudgets, ProjectReviewThresholds, ProjectSourceDeclaration,
        finalize_project_manifest, project_manifest_digest, project_temporal_contract,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

pub const CANON_PROJECT_PLAN_VERSION: &str = "canon.project.plan.v1";

pub fn project_plan_schema_version() -> &'static str {
    CANON_PROJECT_PLAN_VERSION
}

pub type ProjectPlanResult<T> = Result<T, ProjectPlanError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPlanErrorCode {
    ArtifactContract,
    ManifestPolicy,
    LockDrift,
    MissingProducer,
    Cycle,
    IncompatibleMode,
    OutputCollision,
    ReviewPolicy,
    CachePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanDiagnostic {
    pub code: ProjectPlanErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanError {
    pub code: ProjectPlanErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<ProjectPlanDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_command: Option<String>,
}

impl ProjectPlanError {
    pub fn new(code: ProjectPlanErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            diagnostics: Vec::new(),
            next_command: Some("canon project validate --manifest <MANIFEST>".to_string()),
        }
    }

    fn with_diagnostics(
        code: ProjectPlanErrorCode,
        message: impl Into<String>,
        diagnostics: Vec<ProjectPlanDiagnostic>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            diagnostics,
            next_command: Some(
                "canon project plan --manifest <MANIFEST> --lock <LOCK>".to_string(),
            ),
        }
    }
}

impl fmt::Display for ProjectPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProjectPlanError {}

impl From<ProjectManifestError> for ProjectPlanError {
    fn from(error: ProjectManifestError) -> Self {
        Self {
            code: ProjectPlanErrorCode::ManifestPolicy,
            message: error.message,
            diagnostics: vec![ProjectPlanDiagnostic {
                code: ProjectPlanErrorCode::ManifestPolicy,
                node_id: None,
                message: format!("project manifest refused with {:?}", error.code),
            }],
            next_command: Some("canon project validate --manifest <MANIFEST>".to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectPlanRequest {
    pub manifest: ProjectManifest,
    pub lock: ProjectLock,
    pub manifest_path: PathBuf,
    pub lock_path: PathBuf,
    pub plan_artifact_path: Option<PathBuf>,
    pub cache_hits: BTreeSet<String>,
}

impl ProjectPlanRequest {
    pub fn new(
        manifest: ProjectManifest,
        lock: ProjectLock,
        manifest_path: impl Into<PathBuf>,
        lock_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            manifest,
            lock,
            manifest_path: manifest_path.into(),
            lock_path: lock_path.into(),
            plan_artifact_path: None,
            cache_hits: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPlanNodeKind {
    Intake,
    Normalize,
    ExternalMaterialization,
    Index,
    Block,
    Evidence,
    Solve,
    Link,
    Evaluate,
    Review,
    Promote,
    ExactReplay,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPlanNodeClass {
    Computation,
    ExternalMaterialization,
    ReviewPause,
    MutationGate,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPlanCacheDecision {
    Hit,
    Miss,
    NotEligible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPlanSideEffectKind {
    ReadsInput,
    WritesArtifact,
    MayUseNetwork,
    PausesForReview,
    MutatesRegistry,
    ExportsOperatorArtifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPlanOutputMaterialization {
    PlannedArtifact,
    ExternalSnapshot,
    ReviewQueue,
    MutationPreview,
    DeclaredOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanHashRef {
    pub ref_id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanOutput {
    pub output_id: String,
    pub path: String,
    pub content_hash: String,
    pub materialization: ProjectPlanOutputMaterialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanCache {
    pub eligible: bool,
    pub decision: ProjectPlanCacheDecision,
    pub cache_key: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanSideEffect {
    pub kind: ProjectPlanSideEffectKind,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanRefusalCondition {
    pub code: ProjectPlanErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanNode {
    pub node_id: String,
    pub kind: ProjectPlanNodeKind,
    pub class: ProjectPlanNodeClass,
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_hash_inputs: Vec<ProjectPlanHashRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ProjectPlanOutput>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub limits: BTreeMap<String, u64>,
    pub cache: ProjectPlanCache,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub side_effects: Vec<ProjectPlanSideEffect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refusal_conditions: Vec<ProjectPlanRefusalCondition>,
    pub runnable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlanSummary {
    pub total_nodes: usize,
    pub edge_count: usize,
    pub computation_nodes: usize,
    pub external_materialization_nodes: usize,
    pub review_pause_nodes: usize,
    pub mutation_gate_nodes: usize,
    pub export_nodes: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub runnable_nodes: usize,
    pub blocked_nodes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPlan {
    pub schema_version: String,
    pub project_id: String,
    pub plan_kind: String,
    pub manifest_digest: String,
    pub lock_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_artifact_path: Option<String>,
    pub graph_hash: String,
    pub summary: ProjectPlanSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<ProjectPlanNode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub next_commands: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<ProjectPlanDiagnostic>,
}

struct LockCoverage {
    source_hashes: BTreeMap<String, String>,
    package_hashes: BTreeMap<String, String>,
}

struct PlanBuilder<'a> {
    manifest: &'a ProjectManifest,
    manifest_digest: &'a str,
    lock_digest: &'a str,
    manifest_path: &'a Path,
    lock_path: &'a Path,
    plan_artifact_path: Option<&'a Path>,
    cache_hits: &'a BTreeSet<String>,
    coverage: LockCoverage,
    nodes: Vec<ProjectPlanNode>,
    node_outputs: BTreeMap<String, Vec<ProjectPlanOutput>>,
    external_materialization_node: Option<String>,
}

pub fn compile_project_plan(request: ProjectPlanRequest) -> ProjectPlanResult<ProjectPlan> {
    let manifest = finalize_project_manifest(request.manifest)?;
    validate_review_policy(&manifest.review)?;
    validate_plan_modes(&manifest)?;

    let manifest_digest = project_manifest_digest(&manifest)?;
    let lock_digest = project_lock_digest(&request.lock).map_err(|error| ProjectPlanError {
        code: ProjectPlanErrorCode::LockDrift,
        message: error.message,
        diagnostics: vec![ProjectPlanDiagnostic {
            code: ProjectPlanErrorCode::LockDrift,
            node_id: None,
            message: "project lock failed canonical validation".to_string(),
        }],
        next_command: Some(
            "canon project lock refresh --manifest <MANIFEST> --out <LOCK>".to_string(),
        ),
    })?;
    let coverage = ensure_lock_covers_manifest(&manifest, &request.lock, &manifest_digest)?;

    let mut builder = PlanBuilder {
        manifest: &manifest,
        manifest_digest: &manifest_digest,
        lock_digest: &lock_digest,
        manifest_path: &request.manifest_path,
        lock_path: &request.lock_path,
        plan_artifact_path: request.plan_artifact_path.as_deref(),
        cache_hits: &request.cache_hits,
        coverage,
        nodes: Vec::new(),
        node_outputs: BTreeMap::new(),
        external_materialization_node: None,
    };
    builder.build()?;

    let mut plan = ProjectPlan {
        schema_version: CANON_PROJECT_PLAN_VERSION.to_string(),
        project_id: manifest.project_id.clone(),
        plan_kind: "dry_run".to_string(),
        manifest_digest: manifest_digest.clone(),
        lock_digest: lock_digest.clone(),
        plan_artifact_path: request
            .plan_artifact_path
            .as_deref()
            .map(normalized_display_path),
        graph_hash: String::new(),
        summary: empty_summary(),
        nodes: builder.nodes,
        next_commands: BTreeMap::new(),
        diagnostics: Vec::new(),
    };
    finalize_plan(
        &mut plan,
        request.cache_hits,
        request.plan_artifact_path.as_deref(),
    )?;
    Ok(plan)
}

pub fn canonical_project_plan_bytes(plan: &ProjectPlan) -> ProjectPlanResult<Vec<u8>> {
    let canonical = validate_project_plan(plan)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize canonical project plan: {error}"
        ))
    })
}

pub fn render_project_plan_summary(plan: &ProjectPlan) -> String {
    let runnable = if plan.next_commands.is_empty() {
        "none".to_string()
    } else {
        plan.next_commands
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "{} project={} nodes={} edges={} graph={} cache_hits={} runnable={} review_pauses={} mutation_gates={} exports={} next=[{}]",
        plan.schema_version,
        plan.project_id,
        plan.summary.total_nodes,
        plan.summary.edge_count,
        plan.graph_hash,
        plan.summary.cache_hits,
        plan.summary.runnable_nodes,
        plan.summary.review_pause_nodes,
        plan.summary.mutation_gate_nodes,
        plan.summary.export_nodes,
        runnable
    )
}

pub fn validate_project_plan(plan: &ProjectPlan) -> ProjectPlanResult<ProjectPlan> {
    let mut canonical = plan.clone();
    if canonical.schema_version != CANON_PROJECT_PLAN_VERSION {
        return Err(artifact_contract_error(format!(
            "schema_version must equal {CANON_PROJECT_PLAN_VERSION}"
        )));
    }
    if canonical.plan_kind != "dry_run" {
        return Err(artifact_contract_error("plan_kind must equal dry_run"));
    }
    sort_node_internals(&mut canonical.nodes);
    validate_node_ids_and_dependencies(&canonical.nodes)?;
    validate_output_paths(&canonical.nodes, canonical.plan_artifact_path.as_deref())?;

    let expected_hash = compute_graph_hash(&canonical)?;
    if canonical.graph_hash != expected_hash {
        return Err(artifact_contract_error(format!(
            "graph_hash must match canonical graph: expected {expected_hash}, got {}",
            canonical.graph_hash
        )));
    }

    let expected_summary = summarize_nodes(&canonical.nodes);
    if canonical.summary != expected_summary {
        return Err(artifact_contract_error(
            "summary must match canonical project plan nodes",
        ));
    }

    let expected_next = canonical
        .nodes
        .iter()
        .filter(|node| node.runnable)
        .map(|node| (node.node_id.clone(), node.command.clone()))
        .collect::<BTreeMap<_, _>>();
    if canonical.next_commands != expected_next {
        return Err(artifact_contract_error(
            "next_commands must list exactly runnable node commands",
        ));
    }

    Ok(canonical)
}

impl<'a> PlanBuilder<'a> {
    fn build(&mut self) -> ProjectPlanResult<()> {
        self.add_external_materialization_node()?;
        self.add_source_nodes()?;
        self.add_mode_nodes()?;
        Ok(())
    }

    fn add_external_materialization_node(&mut self) -> ProjectPlanResult<()> {
        if self.manifest.runtime.network_policy != ProjectNetworkPolicy::AllowDeclaredHosts {
            return Ok(());
        }
        let node_id = "materialize.external".to_string();
        let inputs = self
            .manifest
            .packages
            .iter()
            .map(|package| {
                hash_ref(
                    format!("package.{}", package.alias),
                    self.coverage
                        .package_hashes
                        .get(&package.alias)
                        .expect("coverage checked")
                        .clone(),
                )
            })
            .chain([hash_value(
                "runtime.declared_hosts",
                &self.manifest.runtime.declared_hosts,
            )?])
            .collect::<Vec<_>>();
        let outputs = vec![output_spec(
            "external_materialization",
            "work/materialize/external.json",
            ProjectPlanOutputMaterialization::ExternalSnapshot,
        )];
        let mut limits = BTreeMap::new();
        limits.insert(
            "declared_host_count".to_string(),
            self.manifest.runtime.declared_hosts.len() as u64,
        );
        self.add_node(NodeSpec {
            node_id: node_id.clone(),
            kind: ProjectPlanNodeKind::ExternalMaterialization,
            class: ProjectPlanNodeClass::ExternalMaterialization,
            dependencies: Vec::new(),
            content_hash_inputs: inputs,
            outputs,
            limits,
            cache_eligible: true,
            side_effects: vec![side_effect(
                ProjectPlanSideEffectKind::MayUseNetwork,
                "materializes declared external package snapshots without provider probing during planning",
            )],
            refusal_conditions: vec![refusal_condition(
                ProjectPlanErrorCode::IncompatibleMode,
                "network execution must stay within runtime.declared_hosts",
                Some("canon project validate --manifest <MANIFEST>"),
            )],
        })?;
        self.external_materialization_node = Some(node_id);
        Ok(())
    }

    fn add_source_nodes(&mut self) -> ProjectPlanResult<()> {
        for source in &self.manifest.sources {
            let source_hash = self
                .coverage
                .source_hashes
                .get(&source.source_id)
                .expect("coverage checked")
                .clone();
            let intake_id = format!("intake.{}", source.source_id);
            self.add_node(NodeSpec {
                node_id: intake_id.clone(),
                kind: ProjectPlanNodeKind::Intake,
                class: ProjectPlanNodeClass::Computation,
                dependencies: Vec::new(),
                content_hash_inputs: vec![
                    hash_ref("manifest", self.manifest_digest.to_string()),
                    hash_ref("lock", self.lock_digest.to_string()),
                    hash_ref(format!("source.{}", source.source_id), source_hash),
                ],
                outputs: vec![output_spec(
                    format!("{}.intake", source.source_id),
                    format!("work/sources/{}/intake.jsonl", source.source_id),
                    ProjectPlanOutputMaterialization::PlannedArtifact,
                )],
                limits: source_limits(source, &self.manifest.budgets),
                cache_eligible: true,
                side_effects: vec![
                    side_effect(
                        ProjectPlanSideEffectKind::ReadsInput,
                        "reads a lock-pinned declared source",
                    ),
                    side_effect(
                        ProjectPlanSideEffectKind::WritesArtifact,
                        "would write an intake artifact during execution",
                    ),
                ],
                refusal_conditions: vec![refusal_condition(
                    ProjectPlanErrorCode::MissingProducer,
                    "source bytes must match the project lock input digest",
                    Some("canon project lock refresh --manifest <MANIFEST> --out <LOCK>"),
                )],
            })?;

            let normalize_id = format!("normalize.{}", source.source_id);
            let dependencies = vec![intake_id.clone()];
            let content_hash_inputs = vec![
                self.node_output_hash_ref(&intake_id, "intake")?,
                hash_ref(
                    format!("package.{}", source.mapping_package),
                    self.coverage
                        .package_hashes
                        .get(&source.mapping_package)
                        .expect("coverage checked")
                        .clone(),
                ),
                hash_value(
                    format!("mapping_profile.{}", source.source_id),
                    &source.mapping_profile,
                )?,
            ];
            self.add_node(NodeSpec {
                node_id: normalize_id,
                kind: ProjectPlanNodeKind::Normalize,
                class: ProjectPlanNodeClass::Computation,
                dependencies,
                content_hash_inputs,
                outputs: vec![output_spec(
                    format!("{}.normalize", source.source_id),
                    format!("work/sources/{}/normalize.jsonl", source.source_id),
                    ProjectPlanOutputMaterialization::PlannedArtifact,
                )],
                limits: source_limits(source, &self.manifest.budgets),
                cache_eligible: true,
                side_effects: vec![side_effect(
                    ProjectPlanSideEffectKind::WritesArtifact,
                    "would write normalized observations during execution",
                )],
                refusal_conditions: vec![refusal_condition(
                    ProjectPlanErrorCode::ManifestPolicy,
                    "source mapping package and profile must be lock-pinned",
                    Some("canon project validate --manifest <MANIFEST>"),
                )],
            })?;
        }
        Ok(())
    }

    fn add_mode_nodes(&mut self) -> ProjectPlanResult<()> {
        for mode in &self.manifest.modes {
            let normalized_dependencies = mode
                .source_ids
                .iter()
                .map(|source_id| format!("normalize.{source_id}"))
                .collect::<Vec<_>>();
            let mut mode_roots = normalized_dependencies.clone();
            if let Some(external) = &self.external_materialization_node {
                mode_roots.push(external.clone());
                mode_roots.sort();
            }

            let mode_inputs = self.mode_inputs(&normalized_dependencies, mode)?;
            let index_id = format!("index.{}", mode.mode_id);
            self.add_node(NodeSpec {
                node_id: index_id.clone(),
                kind: ProjectPlanNodeKind::Index,
                class: ProjectPlanNodeClass::Computation,
                dependencies: mode_roots,
                content_hash_inputs: mode_inputs,
                outputs: vec![output_spec(
                    format!("{}.index", mode.mode_id),
                    format!("work/modes/{}/index/index.json", mode.mode_id),
                    ProjectPlanOutputMaterialization::PlannedArtifact,
                )],
                limits: mode_limits(&self.manifest.budgets),
                cache_eligible: true,
                side_effects: vec![side_effect(
                    ProjectPlanSideEffectKind::WritesArtifact,
                    "would write a reusable index artifact during execution",
                )],
                refusal_conditions: vec![refusal_condition(
                    ProjectPlanErrorCode::CachePolicy,
                    "cache hit is valid only for matching content-hash inputs",
                    Some("canon project plan --manifest <MANIFEST> --lock <LOCK>"),
                )],
            })?;

            let block_id = self.add_linear_mode_node(
                &mode.mode_id,
                ProjectPlanNodeKind::Block,
                ProjectPlanNodeClass::Computation,
                &index_id,
                "block",
                "block/block.json",
                true,
            )?;
            let evidence_id = self.add_linear_mode_node(
                &mode.mode_id,
                ProjectPlanNodeKind::Evidence,
                ProjectPlanNodeClass::Computation,
                &block_id,
                "evidence",
                "evidence/evidence.json",
                true,
            )?;
            let solve_kind = match mode.kind {
                ProjectModeKind::Cluster => ProjectPlanNodeKind::Solve,
                ProjectModeKind::Link => ProjectPlanNodeKind::Link,
            };
            let solve_label = match mode.kind {
                ProjectModeKind::Cluster => "solve",
                ProjectModeKind::Link => "link",
            };
            let solve_id = self.add_linear_mode_node(
                &mode.mode_id,
                solve_kind,
                ProjectPlanNodeClass::Computation,
                &evidence_id,
                solve_label,
                "solve/solve.json",
                true,
            )?;
            let evaluate_id = self.add_linear_mode_node(
                &mode.mode_id,
                ProjectPlanNodeKind::Evaluate,
                ProjectPlanNodeClass::Computation,
                &solve_id,
                "evaluate",
                "evaluate/evaluation.json",
                true,
            )?;
            let review_id = self.add_review_node(&mode.mode_id, &solve_id, &evaluate_id)?;
            let promote_id = self.add_promote_node(&mode.mode_id, &review_id, &evaluate_id)?;
            let replay_id = self.add_linear_mode_node(
                &mode.mode_id,
                ProjectPlanNodeKind::ExactReplay,
                ProjectPlanNodeClass::Computation,
                &promote_id,
                "exact_replay",
                "replay/exact_replay.json",
                true,
            )?;
            for output_id in &mode.output_ids {
                let output = self.output_by_id(output_id)?;
                self.add_export_node(&mode.mode_id, &output, &review_id, &promote_id, &replay_id)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn add_linear_mode_node(
        &mut self,
        mode_id: &str,
        kind: ProjectPlanNodeKind,
        class: ProjectPlanNodeClass,
        dependency: &str,
        label: &str,
        relpath: &str,
        cache_eligible: bool,
    ) -> ProjectPlanResult<String> {
        let node_id = format!("{label}.{mode_id}");
        self.add_node(NodeSpec {
            node_id: node_id.clone(),
            kind,
            class,
            dependencies: vec![dependency.to_string()],
            content_hash_inputs: vec![self.node_output_hash_ref(dependency, label)?],
            outputs: vec![output_spec(
                format!("{mode_id}.{label}"),
                format!("work/modes/{mode_id}/{relpath}"),
                ProjectPlanOutputMaterialization::PlannedArtifact,
            )],
            limits: mode_limits(&self.manifest.budgets),
            cache_eligible,
            side_effects: vec![side_effect(
                ProjectPlanSideEffectKind::WritesArtifact,
                "would write a deterministic stage artifact during execution",
            )],
            refusal_conditions: vec![refusal_condition(
                ProjectPlanErrorCode::MissingProducer,
                "all upstream stage artifacts must be produced or cache-hit",
                Some("canon project plan --manifest <MANIFEST> --lock <LOCK>"),
            )],
        })?;
        Ok(node_id)
    }

    fn add_review_node(
        &mut self,
        mode_id: &str,
        solve_id: &str,
        evaluate_id: &str,
    ) -> ProjectPlanResult<String> {
        let node_id = format!("review.{mode_id}");
        self.add_node(NodeSpec {
            node_id: node_id.clone(),
            kind: ProjectPlanNodeKind::Review,
            class: ProjectPlanNodeClass::ReviewPause,
            dependencies: vec![solve_id.to_string(), evaluate_id.to_string()],
            content_hash_inputs: vec![
                self.node_output_hash_ref(solve_id, "solve")?,
                self.node_output_hash_ref(evaluate_id, "evaluate")?,
                hash_value("review.thresholds", &self.manifest.review)?,
            ],
            outputs: vec![output_spec(
                format!("{mode_id}.review"),
                format!("work/modes/{mode_id}/review/review_queue.csv"),
                ProjectPlanOutputMaterialization::ReviewQueue,
            )],
            limits: review_limits(&self.manifest.budgets),
            cache_eligible: false,
            side_effects: vec![side_effect(
                ProjectPlanSideEffectKind::PausesForReview,
                "requires operator review before mutation gates",
            )],
            refusal_conditions: vec![refusal_condition(
                ProjectPlanErrorCode::ReviewPolicy,
                "review queue must fit max_review_items and preserve the review band",
                Some("canon project validate --manifest <MANIFEST>"),
            )],
        })?;
        Ok(node_id)
    }

    fn add_promote_node(
        &mut self,
        mode_id: &str,
        review_id: &str,
        evaluate_id: &str,
    ) -> ProjectPlanResult<String> {
        let node_id = format!("promote.{mode_id}");
        self.add_node(NodeSpec {
            node_id: node_id.clone(),
            kind: ProjectPlanNodeKind::Promote,
            class: ProjectPlanNodeClass::MutationGate,
            dependencies: vec![review_id.to_string(), evaluate_id.to_string()],
            content_hash_inputs: vec![
                self.node_output_hash_ref(review_id, "review")?,
                self.node_output_hash_ref(evaluate_id, "evaluate")?,
                hash_value("review.thresholds", &self.manifest.review)?,
            ],
            outputs: vec![output_spec(
                format!("{mode_id}.promote"),
                format!("work/modes/{mode_id}/promote/registry_patch.json"),
                ProjectPlanOutputMaterialization::MutationPreview,
            )],
            limits: review_limits(&self.manifest.budgets),
            cache_eligible: false,
            side_effects: vec![side_effect(
                ProjectPlanSideEffectKind::MutatesRegistry,
                "mutation gate is represented in dry-run but never applied by planning",
            )],
            refusal_conditions: vec![refusal_condition(
                ProjectPlanErrorCode::ReviewPolicy,
                "promotion requires accepted review decisions and evaluation evidence",
                Some("canon project review export --plan <PLAN>"),
            )],
        })?;
        Ok(node_id)
    }

    fn add_export_node(
        &mut self,
        mode_id: &str,
        output: &ProjectOutputDeclaration,
        review_id: &str,
        promote_id: &str,
        replay_id: &str,
    ) -> ProjectPlanResult<()> {
        let dependency = match output.kind {
            ProjectOutputKind::ReviewQueueCsv => review_id,
            ProjectOutputKind::RegistrySnapshotJson => promote_id,
            ProjectOutputKind::SummaryJson
            | ProjectOutputKind::ArtifactBundleDir
            | ProjectOutputKind::DiagnosticsJsonl => replay_id,
        };
        let node_id = format!("export.{mode_id}.{}", output.output_id);
        self.add_node(NodeSpec {
            node_id,
            kind: ProjectPlanNodeKind::Export,
            class: ProjectPlanNodeClass::Export,
            dependencies: vec![dependency.to_string()],
            content_hash_inputs: vec![self.node_output_hash_ref(dependency, "export")?],
            outputs: vec![output_spec(
                format!("{mode_id}.{}", output.output_id),
                output.path.clone(),
                ProjectPlanOutputMaterialization::DeclaredOutput,
            )],
            limits: BTreeMap::new(),
            cache_eligible: false,
            side_effects: vec![side_effect(
                ProjectPlanSideEffectKind::ExportsOperatorArtifact,
                "would write an explicitly declared project output during execution",
            )],
            refusal_conditions: vec![refusal_condition(
                ProjectPlanErrorCode::OutputCollision,
                "declared output path must have exactly one producer",
                Some("canon project validate --manifest <MANIFEST>"),
            )],
        })
    }

    fn add_node(&mut self, mut spec: NodeSpec) -> ProjectPlanResult<()> {
        spec.content_hash_inputs
            .push(self.temporal_contract_hash_ref()?);
        spec.content_hash_inputs
            .sort_by(|left, right| left.ref_id.cmp(&right.ref_id));
        spec.content_hash_inputs
            .dedup_by(|left, right| left.ref_id == right.ref_id);
        let command = node_command(
            self.manifest_path,
            self.lock_path,
            self.plan_artifact_path,
            &spec.node_id,
        );
        let cache_key = node_cache_key(&spec)?;
        let decision = if spec.cache_eligible {
            if self.cache_hits.contains(&spec.node_id) {
                ProjectPlanCacheDecision::Hit
            } else {
                ProjectPlanCacheDecision::Miss
            }
        } else {
            ProjectPlanCacheDecision::NotEligible
        };
        let outputs = spec
            .outputs
            .iter()
            .map(|output| ProjectPlanOutput {
                output_id: output.output_id.clone(),
                path: output.path.clone(),
                content_hash: digest_string(format!(
                    "{}|{}|{}",
                    cache_key, output.output_id, output.path
                )),
                materialization: output.materialization,
            })
            .collect::<Vec<_>>();
        let node = ProjectPlanNode {
            node_id: spec.node_id.clone(),
            kind: spec.kind,
            class: spec.class,
            command,
            dependencies: spec.dependencies,
            content_hash_inputs: spec.content_hash_inputs,
            outputs: outputs.clone(),
            limits: spec.limits,
            cache: ProjectPlanCache {
                eligible: spec.cache_eligible,
                decision,
                cache_key,
                reason: if spec.cache_eligible {
                    "content-addressed stage artifact".to_string()
                } else {
                    "operator-visible side effect is not cache-replayable".to_string()
                },
            },
            side_effects: spec.side_effects,
            refusal_conditions: spec.refusal_conditions,
            runnable: false,
            blocked_by: Vec::new(),
        };
        self.node_outputs.insert(spec.node_id, outputs);
        self.nodes.push(node);
        Ok(())
    }

    fn temporal_contract_hash_ref(&self) -> ProjectPlanResult<ProjectPlanHashRef> {
        hash_value(
            "temporal.contract",
            &project_temporal_contract(self.manifest)?,
        )
    }

    fn node_output_hash_ref(
        &self,
        node_id: &str,
        label: &str,
    ) -> ProjectPlanResult<ProjectPlanHashRef> {
        let outputs = self.node_outputs.get(node_id).ok_or_else(|| {
            missing_producer_error(format!(
                "node {node_id} has not been produced before {label}"
            ))
        })?;
        let output = outputs.first().ok_or_else(|| {
            missing_producer_error(format!("node {node_id} does not declare an output"))
        })?;
        Ok(hash_ref(
            format!("node.{node_id}.{}", output.output_id),
            output.content_hash.clone(),
        ))
    }

    fn mode_inputs(
        &self,
        normalized_dependencies: &[String],
        mode: &super::manifest::ProjectExecutionMode,
    ) -> ProjectPlanResult<Vec<ProjectPlanHashRef>> {
        let mut inputs = Vec::new();
        for dependency in normalized_dependencies {
            inputs.push(self.node_output_hash_ref(dependency, "index")?);
        }
        inputs.push(hash_ref(
            format!("package.{}", mode.registry_package),
            self.coverage
                .package_hashes
                .get(&mode.registry_package)
                .expect("coverage checked")
                .clone(),
        ));
        inputs.push(hash_ref(
            format!("package.{}", mode.strategy_package),
            self.coverage
                .package_hashes
                .get(&mode.strategy_package)
                .expect("coverage checked")
                .clone(),
        ));
        inputs.push(hash_ref(
            format!("package.{}", mode.profile_package),
            self.coverage
                .package_hashes
                .get(&mode.profile_package)
                .expect("coverage checked")
                .clone(),
        ));
        inputs.push(hash_value("temporal.scope", &self.manifest.temporal)?);
        inputs.sort_by(|left, right| left.ref_id.cmp(&right.ref_id));
        Ok(inputs)
    }

    fn output_by_id(&self, output_id: &str) -> ProjectPlanResult<ProjectOutputDeclaration> {
        self.manifest
            .outputs
            .iter()
            .find(|output| output.output_id == output_id)
            .cloned()
            .ok_or_else(|| missing_producer_error(format!("output {output_id} is not declared")))
    }
}

#[derive(Debug, Clone, Serialize)]
struct OutputSpec {
    output_id: String,
    path: String,
    materialization: ProjectPlanOutputMaterialization,
}

#[derive(Debug, Clone)]
struct NodeSpec {
    node_id: String,
    kind: ProjectPlanNodeKind,
    class: ProjectPlanNodeClass,
    dependencies: Vec<String>,
    content_hash_inputs: Vec<ProjectPlanHashRef>,
    outputs: Vec<OutputSpec>,
    limits: BTreeMap<String, u64>,
    cache_eligible: bool,
    side_effects: Vec<ProjectPlanSideEffect>,
    refusal_conditions: Vec<ProjectPlanRefusalCondition>,
}

fn ensure_lock_covers_manifest(
    manifest: &ProjectManifest,
    lock: &ProjectLock,
    manifest_digest: &str,
) -> ProjectPlanResult<LockCoverage> {
    if lock.project_id != manifest.project_id {
        return Err(lock_drift_error(format!(
            "lock project_id {} does not match manifest project_id {}",
            lock.project_id, manifest.project_id
        )));
    }
    if lock.project_digest != manifest_digest {
        return Err(lock_drift_error(format!(
            "lock project_digest {} does not match current manifest digest {}",
            lock.project_digest, manifest_digest
        )));
    }

    let inputs = lock
        .inputs
        .iter()
        .map(|input| (input.input_id.as_str(), input))
        .collect::<BTreeMap<_, _>>();
    let refs = lock
        .resolved_refs
        .iter()
        .map(|resolved| (resolved.ref_id.as_str(), resolved))
        .collect::<BTreeMap<_, _>>();

    let mut source_hashes = BTreeMap::new();
    for source in &manifest.sources {
        let input = inputs.get(source.source_id.as_str()).ok_or_else(|| {
            missing_producer_error(format!("lock is missing source input {}", source.source_id))
        })?;
        if input.relative_path != source.path {
            return Err(lock_drift_error(format!(
                "lock input {} path {} does not match manifest path {}",
                source.source_id, input.relative_path, source.path
            )));
        }
        source_hashes.insert(source.source_id.clone(), input.content_digest.clone());
    }

    let mut package_hashes = BTreeMap::new();
    for package in &manifest.packages {
        let resolved = refs.get(package.alias.as_str()).ok_or_else(|| {
            missing_producer_error(format!("lock is missing package alias {}", package.alias))
        })?;
        let expected_kind = lock_kind_for_package(package);
        if resolved.kind != expected_kind {
            return Err(lock_drift_error(format!(
                "lock ref {} has kind {:?}, expected {:?}",
                package.alias, resolved.kind, expected_kind
            )));
        }
        if resolved.resolved_digest != package.content_hash {
            return Err(lock_drift_error(format!(
                "lock ref {} digest {} does not match manifest package hash {}",
                package.alias, resolved.resolved_digest, package.content_hash
            )));
        }
        package_hashes.insert(package.alias.clone(), resolved.resolved_digest.clone());
    }

    Ok(LockCoverage {
        source_hashes,
        package_hashes,
    })
}

fn validate_review_policy(review: &ProjectReviewThresholds) -> ProjectPlanResult<()> {
    if review.review_required_min_score_basis_points >= review.auto_promote_min_score_basis_points {
        return Err(ProjectPlanError::new(
            ProjectPlanErrorCode::ReviewPolicy,
            "project plan requires a non-empty review band before the mutation gate",
        ));
    }
    Ok(())
}

fn validate_plan_modes(manifest: &ProjectManifest) -> ProjectPlanResult<()> {
    for mode in &manifest.modes {
        if mode.kind == ProjectModeKind::Link && mode.source_ids.len() != 2 {
            return Err(ProjectPlanError::new(
                ProjectPlanErrorCode::IncompatibleMode,
                format!(
                    "project plan supports link modes with exactly two sources; {} has {}",
                    mode.mode_id,
                    mode.source_ids.len()
                ),
            ));
        }
    }
    Ok(())
}

fn finalize_plan(
    plan: &mut ProjectPlan,
    cache_hits: BTreeSet<String>,
    plan_artifact_path: Option<&Path>,
) -> ProjectPlanResult<()> {
    sort_node_internals(&mut plan.nodes);
    validate_requested_cache_hits(&plan.nodes, &cache_hits)?;
    validate_node_ids_and_dependencies(&plan.nodes)?;
    validate_output_paths(
        &plan.nodes,
        plan_artifact_path.map(normalized_display_path).as_deref(),
    )?;
    apply_runnable_state(&mut plan.nodes);
    plan.summary = summarize_nodes(&plan.nodes);
    plan.next_commands = plan
        .nodes
        .iter()
        .filter(|node| node.runnable)
        .map(|node| (node.node_id.clone(), node.command.clone()))
        .collect();
    plan.graph_hash = compute_graph_hash(plan)?;
    Ok(())
}

fn validate_requested_cache_hits(
    nodes: &[ProjectPlanNode],
    cache_hits: &BTreeSet<String>,
) -> ProjectPlanResult<()> {
    let by_id = nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = Vec::new();
    for hit in cache_hits {
        match by_id.get(hit.as_str()) {
            Some(node) if node.cache.eligible => {}
            Some(_) => diagnostics.push(ProjectPlanDiagnostic {
                code: ProjectPlanErrorCode::CachePolicy,
                node_id: Some(hit.clone()),
                message: format!("{hit} is not cache eligible"),
            }),
            None => diagnostics.push(ProjectPlanDiagnostic {
                code: ProjectPlanErrorCode::CachePolicy,
                node_id: Some(hit.clone()),
                message: format!("{hit} is not a project plan node"),
            }),
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(ProjectPlanError::with_diagnostics(
            ProjectPlanErrorCode::CachePolicy,
            "requested cache hits are not valid for this plan",
            diagnostics,
        ))
    }
}

fn validate_node_ids_and_dependencies(nodes: &[ProjectPlanNode]) -> ProjectPlanResult<()> {
    let mut ids = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for node in nodes {
        if node.node_id.trim().is_empty() {
            diagnostics.push(ProjectPlanDiagnostic {
                code: ProjectPlanErrorCode::ArtifactContract,
                node_id: None,
                message: "node_id must be non-empty".to_string(),
            });
        }
        if !ids.insert(node.node_id.as_str()) {
            diagnostics.push(ProjectPlanDiagnostic {
                code: ProjectPlanErrorCode::ArtifactContract,
                node_id: Some(node.node_id.clone()),
                message: "duplicate node_id".to_string(),
            });
        }
    }
    for node in nodes {
        for dependency in &node.dependencies {
            if !ids.contains(dependency.as_str()) {
                diagnostics.push(ProjectPlanDiagnostic {
                    code: ProjectPlanErrorCode::MissingProducer,
                    node_id: Some(node.node_id.clone()),
                    message: format!("dependency {dependency} has no producer node"),
                });
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(ProjectPlanError::with_diagnostics(
            ProjectPlanErrorCode::MissingProducer,
            "project plan contains dependencies without producers",
            diagnostics,
        ));
    }
    detect_cycles(nodes)
}

fn detect_cycles(nodes: &[ProjectPlanNode]) -> ProjectPlanResult<()> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }

    fn visit<'a>(
        node_id: &'a str,
        by_id: &BTreeMap<&'a str, &'a ProjectPlanNode>,
        marks: &mut BTreeMap<&'a str, Mark>,
        stack: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        if matches!(marks.get(node_id), Some(Mark::Done)) {
            return None;
        }
        if matches!(marks.get(node_id), Some(Mark::Visiting)) {
            let start = stack
                .iter()
                .position(|existing| *existing == node_id)
                .unwrap_or(0);
            return Some(
                stack[start..]
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect(),
            );
        }
        marks.insert(node_id, Mark::Visiting);
        stack.push(node_id);
        if let Some(node) = by_id.get(node_id) {
            for dependency in &node.dependencies {
                if let Some(cycle) = visit(dependency, by_id, marks, stack) {
                    return Some(cycle);
                }
            }
        }
        stack.pop();
        marks.insert(node_id, Mark::Done);
        None
    }

    let by_id = nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut marks = BTreeMap::new();
    for node_id in by_id.keys() {
        if let Some(cycle) = visit(node_id, &by_id, &mut marks, &mut Vec::new()) {
            return Err(ProjectPlanError::with_diagnostics(
                ProjectPlanErrorCode::Cycle,
                "project plan dependency graph contains a cycle",
                vec![ProjectPlanDiagnostic {
                    code: ProjectPlanErrorCode::Cycle,
                    node_id: Some((*node_id).to_string()),
                    message: cycle.join(" -> "),
                }],
            ));
        }
    }
    Ok(())
}

fn validate_output_paths(
    nodes: &[ProjectPlanNode],
    plan_artifact_path: Option<&str>,
) -> ProjectPlanResult<()> {
    let mut seen = BTreeMap::<String, String>::new();
    let mut diagnostics = Vec::new();
    if let Some(path) = plan_artifact_path {
        seen.insert(path.to_string(), "plan_artifact".to_string());
    }
    for node in nodes {
        for output in &node.outputs {
            if let Some(previous) = seen.insert(output.path.clone(), node.node_id.clone()) {
                diagnostics.push(ProjectPlanDiagnostic {
                    code: ProjectPlanErrorCode::OutputCollision,
                    node_id: Some(node.node_id.clone()),
                    message: format!(
                        "output path {} is claimed by both {} and {}",
                        output.path, previous, node.node_id
                    ),
                });
            }
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(ProjectPlanError::with_diagnostics(
            ProjectPlanErrorCode::OutputCollision,
            "project plan contains output path collisions",
            diagnostics,
        ))
    }
}

fn apply_runnable_state(nodes: &mut [ProjectPlanNode]) {
    let mut satisfied = nodes
        .iter()
        .filter(|node| node.cache.decision == ProjectPlanCacheDecision::Hit)
        .map(|node| node.node_id.clone())
        .collect::<BTreeSet<_>>();

    for node in nodes {
        node.runnable = false;
        node.blocked_by.clear();
        if node.cache.decision == ProjectPlanCacheDecision::Hit {
            continue;
        }
        let missing = node
            .dependencies
            .iter()
            .filter(|dependency| !satisfied.contains(*dependency))
            .cloned()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            if node.class == ProjectPlanNodeClass::MutationGate {
                node.blocked_by.push("operator_approval".to_string());
            } else {
                node.runnable = true;
            }
        } else {
            node.blocked_by = missing;
        }
        if node.cache.decision == ProjectPlanCacheDecision::Hit {
            satisfied.insert(node.node_id.clone());
        }
    }
}

fn summarize_nodes(nodes: &[ProjectPlanNode]) -> ProjectPlanSummary {
    ProjectPlanSummary {
        total_nodes: nodes.len(),
        edge_count: nodes.iter().map(|node| node.dependencies.len()).sum(),
        computation_nodes: nodes
            .iter()
            .filter(|node| node.class == ProjectPlanNodeClass::Computation)
            .count(),
        external_materialization_nodes: nodes
            .iter()
            .filter(|node| node.class == ProjectPlanNodeClass::ExternalMaterialization)
            .count(),
        review_pause_nodes: nodes
            .iter()
            .filter(|node| node.class == ProjectPlanNodeClass::ReviewPause)
            .count(),
        mutation_gate_nodes: nodes
            .iter()
            .filter(|node| node.class == ProjectPlanNodeClass::MutationGate)
            .count(),
        export_nodes: nodes
            .iter()
            .filter(|node| node.class == ProjectPlanNodeClass::Export)
            .count(),
        cache_hits: nodes
            .iter()
            .filter(|node| node.cache.decision == ProjectPlanCacheDecision::Hit)
            .count(),
        cache_misses: nodes
            .iter()
            .filter(|node| node.cache.decision == ProjectPlanCacheDecision::Miss)
            .count(),
        runnable_nodes: nodes.iter().filter(|node| node.runnable).count(),
        blocked_nodes: nodes
            .iter()
            .filter(|node| !node.blocked_by.is_empty())
            .count(),
    }
}

fn empty_summary() -> ProjectPlanSummary {
    ProjectPlanSummary {
        total_nodes: 0,
        edge_count: 0,
        computation_nodes: 0,
        external_materialization_nodes: 0,
        review_pause_nodes: 0,
        mutation_gate_nodes: 0,
        export_nodes: 0,
        cache_hits: 0,
        cache_misses: 0,
        runnable_nodes: 0,
        blocked_nodes: 0,
    }
}

fn compute_graph_hash(plan: &ProjectPlan) -> ProjectPlanResult<String> {
    #[derive(Serialize)]
    struct GraphNode<'a> {
        node_id: &'a str,
        kind: ProjectPlanNodeKind,
        class: ProjectPlanNodeClass,
        command: &'a str,
        dependencies: &'a [String],
        content_hash_inputs: &'a [ProjectPlanHashRef],
        outputs: &'a [ProjectPlanOutput],
        limits: &'a BTreeMap<String, u64>,
        cache_eligible: bool,
        cache_key: &'a str,
        side_effects: &'a [ProjectPlanSideEffect],
        refusal_conditions: &'a [ProjectPlanRefusalCondition],
    }
    #[derive(Serialize)]
    struct Graph<'a> {
        schema_version: &'a str,
        project_id: &'a str,
        plan_kind: &'a str,
        manifest_digest: &'a str,
        lock_digest: &'a str,
        plan_artifact_path: &'a Option<String>,
        nodes: Vec<GraphNode<'a>>,
    }

    let graph = Graph {
        schema_version: &plan.schema_version,
        project_id: &plan.project_id,
        plan_kind: &plan.plan_kind,
        manifest_digest: &plan.manifest_digest,
        lock_digest: &plan.lock_digest,
        plan_artifact_path: &plan.plan_artifact_path,
        nodes: plan
            .nodes
            .iter()
            .map(|node| GraphNode {
                node_id: &node.node_id,
                kind: node.kind,
                class: node.class,
                command: &node.command,
                dependencies: &node.dependencies,
                content_hash_inputs: &node.content_hash_inputs,
                outputs: &node.outputs,
                limits: &node.limits,
                cache_eligible: node.cache.eligible,
                cache_key: &node.cache.cache_key,
                side_effects: &node.side_effects,
                refusal_conditions: &node.refusal_conditions,
            })
            .collect(),
    };
    serde_json::to_vec(&graph)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| {
            artifact_contract_error(format!("failed to serialize project graph hash: {error}"))
        })
}

fn node_cache_key(spec: &NodeSpec) -> ProjectPlanResult<String> {
    #[derive(Serialize)]
    struct CacheMaterial<'a> {
        node_id: &'a str,
        kind: ProjectPlanNodeKind,
        class: ProjectPlanNodeClass,
        dependencies: &'a [String],
        content_hash_inputs: &'a [ProjectPlanHashRef],
        outputs: &'a [OutputSpec],
        limits: &'a BTreeMap<String, u64>,
    }
    let material = CacheMaterial {
        node_id: &spec.node_id,
        kind: spec.kind,
        class: spec.class,
        dependencies: &spec.dependencies,
        content_hash_inputs: &spec.content_hash_inputs,
        outputs: &spec.outputs,
        limits: &spec.limits,
    };
    serde_json::to_vec(&material)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| artifact_contract_error(format!("failed to build cache key: {error}")))
}

fn sort_node_internals(nodes: &mut [ProjectPlanNode]) {
    for node in nodes {
        node.dependencies.sort();
        node.dependencies.dedup();
        node.content_hash_inputs
            .sort_by(|left, right| left.ref_id.cmp(&right.ref_id));
        node.outputs
            .sort_by(|left, right| left.output_id.cmp(&right.output_id));
        node.side_effects.sort_by_key(|effect| effect.kind);
        node.refusal_conditions.sort_by(|left, right| {
            left.code
                .cmp(&right.code)
                .then(left.message.cmp(&right.message))
        });
        node.blocked_by.sort();
        node.blocked_by.dedup();
    }
}

fn source_limits(
    source: &ProjectSourceDeclaration,
    budgets: &ProjectResourceBudgets,
) -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("max_input_bytes".to_string(), budgets.max_input_bytes),
        ("max_rows".to_string(), budgets.max_rows),
        ("required".to_string(), u64::from(source.required)),
    ])
}

fn mode_limits(budgets: &ProjectResourceBudgets) -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("max_candidates".to_string(), budgets.max_candidates),
        ("max_rows".to_string(), budgets.max_rows),
        (
            "max_runtime_seconds".to_string(),
            budgets.max_runtime_seconds,
        ),
    ])
}

fn review_limits(budgets: &ProjectResourceBudgets) -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("max_review_items".to_string(), budgets.max_review_items),
        (
            "max_runtime_seconds".to_string(),
            budgets.max_runtime_seconds,
        ),
    ])
}

fn output_spec(
    output_id: impl Into<String>,
    path: impl Into<String>,
    materialization: ProjectPlanOutputMaterialization,
) -> OutputSpec {
    OutputSpec {
        output_id: output_id.into(),
        path: path.into(),
        materialization,
    }
}

fn hash_ref(ref_id: impl Into<String>, content_hash: String) -> ProjectPlanHashRef {
    ProjectPlanHashRef {
        ref_id: ref_id.into(),
        content_hash,
    }
}

fn hash_value<T: Serialize>(
    ref_id: impl Into<String>,
    value: &T,
) -> ProjectPlanResult<ProjectPlanHashRef> {
    serde_json::to_vec(value)
        .map(|bytes| hash_ref(ref_id, digest_bytes(&bytes)))
        .map_err(|error| artifact_contract_error(format!("failed to hash plan input: {error}")))
}

fn side_effect(
    kind: ProjectPlanSideEffectKind,
    description: impl Into<String>,
) -> ProjectPlanSideEffect {
    ProjectPlanSideEffect {
        kind,
        description: description.into(),
    }
}

fn refusal_condition(
    code: ProjectPlanErrorCode,
    message: impl Into<String>,
    next_command: Option<&str>,
) -> ProjectPlanRefusalCondition {
    ProjectPlanRefusalCondition {
        code,
        message: message.into(),
        next_command: next_command.map(str::to_string),
    }
}

fn lock_kind_for_package(package: &ProjectPackageBinding) -> ProjectLockRefKind {
    match package.kind {
        ProjectPackageKind::Strategy => ProjectLockRefKind::Strategy,
        ProjectPackageKind::Registry
        | ProjectPackageKind::EntityProfile
        | ProjectPackageKind::SourceMapping
        | ProjectPackageKind::Extension => ProjectLockRefKind::Package,
    }
}

fn node_command(
    manifest_path: &Path,
    lock_path: &Path,
    plan_artifact_path: Option<&Path>,
    node_id: &str,
) -> String {
    if let Some(plan_path) = plan_artifact_path {
        return format!(
            "canon project execute --plan {} --node {}",
            shell_token(&normalized_display_path(plan_path)),
            shell_token(node_id)
        );
    }
    format!(
        "canon project execute --manifest {} --lock {} --node {}",
        shell_token(&normalized_display_path(manifest_path)),
        shell_token(&normalized_display_path(lock_path)),
        shell_token(node_id)
    )
}

fn normalized_display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn shell_token(value: &str) -> String {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"./_:-".contains(&byte))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn digest_string(value: impl AsRef<str>) -> String {
    digest_bytes(value.as_ref().as_bytes())
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn artifact_contract_error(message: impl Into<String>) -> ProjectPlanError {
    ProjectPlanError::new(ProjectPlanErrorCode::ArtifactContract, message)
}

fn lock_drift_error(message: impl Into<String>) -> ProjectPlanError {
    ProjectPlanError {
        code: ProjectPlanErrorCode::LockDrift,
        message: message.into(),
        diagnostics: Vec::new(),
        next_command: Some(
            "canon project lock refresh --manifest <MANIFEST> --out <LOCK>".to_string(),
        ),
    }
}

fn missing_producer_error(message: impl Into<String>) -> ProjectPlanError {
    ProjectPlanError {
        code: ProjectPlanErrorCode::MissingProducer,
        message: message.into(),
        diagnostics: Vec::new(),
        next_command: Some(
            "canon project lock refresh --manifest <MANIFEST> --out <LOCK>".to_string(),
        ),
    }
}
