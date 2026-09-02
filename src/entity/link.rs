#![forbid(unsafe_code)]

//! Directional entity-link adapter over the shared native entity stages.
//!
//! Link mode keeps reference/target semantics in the materialized input rows,
//! then delegates indexing, blocking, evidence, solve, review, audit, promote,
//! and apply handoffs to `run_entity_workbench`.

use super::{
    EntityRunArtifact, EntityRunRequest, EntityRunResult,
    publish_entity_run_link_publication_patch,
    read_entity_run_committed_publication_stable_path_bytes, run_entity_workbench_with_cache_mode,
};
use crate::{
    InputFormat, Refusal,
    entity::diagnostics::{
        EntityUnlinkablesReport, EntityUnlinkablesReportRequest, EntityUnlinkablesSurfaceInput,
        EntityUnlinkablesSurfaceSide, EntityUnlinkablesThresholds, build_entity_unlinkables_report,
        validate_entity_unlinkables_report,
    },
    entity::evidence_ir::{
        EvidenceBundle, canonical_bundle_bytes as canonical_evidence_bundle_bytes,
    },
    entity::index::EntityIndexCacheMode,
    entity::publication::{CANON_ENTITY_STAGE_PUBLICATION_VERSION, EntityPublicationFileInput},
    entity::record_link::{
        ASSIGNMENT_ALIGNMENT_PATH, ASSIGNMENT_ALIGNMENT_VERSION, AssignmentAlignmentSidecar,
        RECORD_LINK_EVIDENCE_PATH, RecordLinkFeaturePolicy, RecordLinkFeatureValue,
        RecordLinkInputSet, RecordLinkLoadRequest, canonical_assignment_alignment_bytes,
        load_record_link_inputs, validate_assignment_alignment_sidecar,
    },
    entity::source_mapping::{
        RecordLinkComparisonView, RecordLinkFieldDispositionReason, RecordLinkInputRecord,
        RecordLinkQuarantinedField,
    },
    entity::{
        CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactReference,
        error::EntityRefusalKind,
        prepare::{
            LoadedPrepareProfile, PrepareInputContract, PreparedInputObservation,
            PreparedSurfaceRecord, load_prepare_profile_with_hash, project_prepare_csv_reader,
        },
    },
    input,
    resolve::{
        AmbiguousRecord, MatchRecord, ResolveRegistrySnapshot, ResolveSummary, StrategyReference,
        TapeSummary, UnmatchedRecord,
    },
    witness,
};
use csv::{ReaderBuilder, WriterBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufRead, BufReader, Cursor},
    path::{Component, Path, PathBuf},
};

pub mod multisource;

pub const ENTITY_LINK_VERSION: &str = "canon_entity_link.v1";
pub const ENTITY_LINK_DECISIONS_VERSION: &str = "canon_entity_link_decisions.v1";
pub const ENTITY_LINK_MATERIALIZED_ROWS_VERSION: &str = "canon_entity_link_materialized_rows.v1";
pub const ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION: &str =
    "canon_entity_link_observation_surface_bindings.v1";
pub const LINK_SIDE_COLUMN: &str = "canon_link_side";
pub const LINK_SOURCE_NAME_COLUMN: &str = "canon_link_source_name";
pub const LINK_SOURCE_ROW_COLUMN: &str = "canon_link_source_row_id";
pub const LINK_SOURCE_ORDINAL_COLUMN: &str = "canon_link_source_ordinal";
pub const LINK_ARTIFACT_PATH: &str = "link/link.json";
pub const LINK_MATERIALIZED_ROWS_PATH: &str = "combined_rows.csv";
pub const LINK_OBSERVATION_SURFACE_BINDINGS_PATH: &str = "observation_surface_bindings.jsonl";
pub const LINK_ASSIGNMENT_ALIGNMENT_PATH: &str = "assignment_alignment.json";
const LINK_COMPOSITE_ID_SEPARATOR: &str = "|";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityLinkRequest<'a> {
    pub reference_rows: &'a Path,
    pub target_rows: &'a Path,
    pub profile: &'a str,
    pub strategy: &'a Path,
    pub registry: &'a Path,
    pub work_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityLinkResult {
    pub artifact: EntityLinkArtifact,
    pub run: EntityRunResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityLinkFinalizeRequest<'a> {
    pub artifact: EntityLinkArtifact,
    pub run_artifact: &'a EntityRunArtifact,
    pub decisions: &'a crate::resolve::ResolveArtifact,
    pub work_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityLinkArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: crate::entity::EntityArtifactMetadata,
    pub summary: ResolveSummary,
    pub mode: EntityLinkMode,
    pub reference: EntityLinkInput,
    pub target: EntityLinkInput,
    pub materialized_rows_path: String,
    pub materialized_rows_content_hash: String,
    pub profile_source: EntityLinkProfileSource,
    pub observation_surface_bindings_path: String,
    pub observation_surface_bindings_content_hash: String,
    pub shared_run_artifact: EntityArtifactReference,
    pub shared_solve_artifact: EntityArtifactReference,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignment_alignment_artifacts: Vec<EntityLinkAssignmentAlignmentArtifact>,
    pub decision_artifact: EntityLinkDecisionArtifact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unlinkables: Option<EntityUnlinkablesReport>,
    pub next_commands: EntityLinkNextCommands,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLinkProfileSource {
    pub source: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLinkAssignmentAlignmentArtifact {
    pub version: String,
    pub path: String,
    pub content_hash: String,
    pub evidence_semantics: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLinkObservationSurfaceBinding {
    pub version: String,
    pub side: EntityLinkRole,
    pub link_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_row_id: Option<String>,
    pub source_ordinal: u64,
    pub surface_id: String,
    pub profile_id: String,
    pub surface_binding_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityLinkDecisionArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub strategy: StrategyReference,
    pub registry: ResolveRegistrySnapshot,
    pub reference_tape: TapeSummary,
    pub target_tape: TapeSummary,
    pub summary: ResolveSummary,
    pub matches: Vec<MatchRecord>,
    pub unmatched: Vec<UnmatchedRecord>,
    pub ambiguous: Vec<AmbiguousRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflict_warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gold_score: Option<crate::resolve::GoldScore>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_back: Option<crate::resolve::WriteBackSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLinkNextCommands {
    pub review_export: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityLinkMode {
    DirectionalTwoTape,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLinkInput {
    pub role: EntityLinkRole,
    pub rows_path: String,
    pub row_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityLinkRole {
    Reference,
    Target,
}

impl EntityLinkRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Target => "target",
        }
    }
}

pub fn run_entity_link(request: EntityLinkRequest<'_>) -> Result<EntityLinkResult, Refusal> {
    run_entity_link_with_cache_mode(request, EntityIndexCacheMode::Enabled)
}

pub fn run_entity_link_with_cache_mode(
    request: EntityLinkRequest<'_>,
    cache_mode: EntityIndexCacheMode,
) -> Result<EntityLinkResult, Refusal> {
    let materialized_rows = materialized_rows_path(request.work_dir);
    let materialized = materialize_directional_rows(
        request.reference_rows,
        request.target_rows,
        &materialized_rows,
    )?;
    let run = run_entity_workbench_with_cache_mode(
        EntityRunRequest {
            rows: &materialized_rows,
            profile: request.profile,
            strategy: request.strategy,
            registry: request.registry,
            work_dir: request.work_dir,
        },
        cache_mode,
    )?;
    let shared_run_artifact = EntityArtifactReference {
        version: run.artifact.version.clone(),
        content_hash: run.artifact.artifact_content_hash.clone(),
    };
    let shared_solve_artifact = solve_stage_reference(&run.artifact)?;
    let shared_publication_artifact = EntityArtifactReference {
        version: CANON_ENTITY_STAGE_PUBLICATION_VERSION.to_string(),
        content_hash: run.publication.generation_id.clone(),
    };
    let mut metadata = run.artifact.metadata.clone();
    metadata.upstream_artifacts = vec![
        shared_run_artifact.clone(),
        shared_solve_artifact.clone(),
        shared_publication_artifact,
    ];
    metadata.upstream_artifacts.sort_by(artifact_ref_cmp);
    metadata.artifact_content_hash.clear();
    let profile_source = link_profile_source_from_request(request.profile, &run.artifact)?;
    let artifact = EntityLinkArtifact {
        version: ENTITY_LINK_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary: ResolveSummary {
            target_records: materialized.target_rows as usize,
            matched: 0,
            unmatched: materialized.target_rows as usize,
            ambiguous: 0,
            match_rate: 0.0,
        },
        mode: EntityLinkMode::DirectionalTwoTape,
        reference: EntityLinkInput {
            role: EntityLinkRole::Reference,
            rows_path: request.reference_rows.display().to_string(),
            row_count: materialized.reference_rows,
        },
        target: EntityLinkInput {
            role: EntityLinkRole::Target,
            rows_path: request.target_rows.display().to_string(),
            row_count: materialized.target_rows,
        },
        materialized_rows_path: LINK_MATERIALIZED_ROWS_PATH.to_string(),
        materialized_rows_content_hash: String::new(),
        profile_source,
        observation_surface_bindings_path: LINK_OBSERVATION_SURFACE_BINDINGS_PATH.to_string(),
        observation_surface_bindings_content_hash: String::new(),
        shared_run_artifact,
        shared_solve_artifact,
        assignment_alignment_artifacts: Vec::new(),
        decision_artifact: empty_link_decision_artifact(),
        unlinkables: None,
        next_commands: EntityLinkNextCommands {
            review_export: format!(
                "canon entity review export {} --include escrow --emit csv",
                request.work_dir.join(LINK_ARTIFACT_PATH).display()
            ),
        },
    };
    Ok(EntityLinkResult { artifact, run })
}

pub fn materialized_rows_path(work_dir: &Path) -> PathBuf {
    work_dir.join("link").join(LINK_MATERIALIZED_ROWS_PATH)
}

pub fn observation_surface_bindings_path(work_dir: &Path) -> PathBuf {
    work_dir
        .join("link")
        .join(LINK_OBSERVATION_SURFACE_BINDINGS_PATH)
}

pub fn link_artifact_path(work_dir: &Path) -> PathBuf {
    work_dir.join(LINK_ARTIFACT_PATH)
}

fn link_materialized_rows_publication_path() -> String {
    format!("link/{LINK_MATERIALIZED_ROWS_PATH}")
}

fn link_observation_surface_bindings_publication_path() -> String {
    format!("link/{LINK_OBSERVATION_SURFACE_BINDINGS_PATH}")
}

fn link_assignment_alignment_publication_path() -> String {
    format!("link/{LINK_ASSIGNMENT_ALIGNMENT_PATH}")
}

pub fn finalize_entity_link_artifact(
    request: EntityLinkFinalizeRequest<'_>,
) -> Result<EntityLinkArtifact, Refusal> {
    let mut artifact = request.artifact;
    let expected_parent_generation_id = link_publication_parent_generation_id(&artifact)?;
    let shared_run_artifact = EntityArtifactReference {
        version: request.run_artifact.version.clone(),
        content_hash: request.run_artifact.artifact_content_hash.clone(),
    };
    let shared_solve_artifact = solve_stage_reference(request.run_artifact)?;
    let shared_publication_artifact = EntityArtifactReference {
        version: CANON_ENTITY_STAGE_PUBLICATION_VERSION.to_string(),
        content_hash: expected_parent_generation_id.clone(),
    };
    artifact.shared_run_artifact = shared_run_artifact.clone();
    artifact.shared_solve_artifact = shared_solve_artifact.clone();
    let (decision_artifact, canonical_summary) =
        link_decision_artifact_and_summary(request.decisions)?;
    artifact.summary = canonical_summary;
    artifact.materialized_rows_path = LINK_MATERIALIZED_ROWS_PATH.to_string();
    let materialized_rows_bytes = read_link_stable_bytes(
        &materialized_rows_path(request.work_dir),
        "materialized rows",
    )?;
    artifact.materialized_rows_content_hash = witness::hash_bytes(&materialized_rows_bytes);
    artifact.observation_surface_bindings_path = LINK_OBSERVATION_SURFACE_BINDINGS_PATH.to_string();
    let mut metadata = request.run_artifact.metadata.clone();
    metadata.upstream_artifacts = vec![
        shared_run_artifact.clone(),
        shared_solve_artifact.clone(),
        shared_publication_artifact,
    ];
    metadata.upstream_artifacts.sort_by(artifact_ref_cmp);
    metadata.artifact_content_hash.clear();
    artifact.metadata = metadata;
    let (assignment_alignment_artifacts, assignment_alignment_bytes) =
        load_link_assignment_alignment_artifacts(request.work_dir)?;
    artifact.assignment_alignment_artifacts = assignment_alignment_artifacts;
    artifact.decision_artifact = decision_artifact;
    let profile_context_dirs =
        link_profile_source_context_dirs(&link_artifact_path(request.work_dir));
    let loaded_profile = validate_link_profile_source_against_run(
        &artifact.profile_source,
        &profile_context_dirs,
        request.run_artifact,
    )?;
    let bindings = build_link_observation_surface_bindings(
        request.work_dir,
        &artifact,
        &profile_context_dirs,
        request.run_artifact,
        request.decisions,
    )?;
    validate_entity_link_observation_surface_bindings(&artifact, &bindings)?;
    let bindings_bytes = jsonl_bytes(&bindings, "observation/surface bindings")?;
    artifact.observation_surface_bindings_content_hash = witness::hash_bytes(&bindings_bytes);
    let surfaces = read_link_run_surfaces(request.work_dir, request.run_artifact, &loaded_profile)?;
    artifact.unlinkables = Some(build_link_unlinkables_report(
        request.work_dir,
        request.run_artifact,
        &loaded_profile,
        &profile_context_dirs,
        &bindings,
        &surfaces,
    )?);
    artifact.next_commands = EntityLinkNextCommands {
        review_export: format!(
            "canon entity review export {} --include escrow --emit csv",
            link_artifact_path(request.work_dir).display()
        ),
    };
    artifact.artifact_content_hash = hash_link_artifact_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    validate_entity_link_artifact_contract(&artifact)?;
    let artifact_bytes = json_bytes(&artifact, "link artifact")?;
    let mut publication_files = vec![
        EntityPublicationFileInput::new(
            link_materialized_rows_publication_path(),
            "link",
            ENTITY_LINK_MATERIALIZED_ROWS_VERSION,
            materialized_rows_bytes,
        ),
        EntityPublicationFileInput::new(
            link_observation_surface_bindings_publication_path(),
            "link",
            ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION,
            bindings_bytes,
        ),
        EntityPublicationFileInput::new(
            LINK_ARTIFACT_PATH,
            "link",
            ENTITY_LINK_VERSION,
            artifact_bytes,
        ),
    ];
    if let Some(bytes) = assignment_alignment_bytes {
        publication_files.push(EntityPublicationFileInput::new(
            link_assignment_alignment_publication_path(),
            "link",
            ASSIGNMENT_ALIGNMENT_VERSION,
            bytes,
        ));
    }
    publish_entity_run_link_publication_patch(
        request.work_dir,
        &expected_parent_generation_id,
        vec![shared_run_artifact, shared_solve_artifact],
        publication_files,
    )?;
    Ok(artifact)
}

fn build_link_unlinkables_report(
    _work_dir: &Path,
    run_artifact: &EntityRunArtifact,
    loaded_profile: &LoadedPrepareProfile,
    profile_context_dirs: &[PathBuf],
    bindings: &[EntityLinkObservationSurfaceBinding],
    surfaces: &[PreparedSurfaceRecord],
) -> Result<EntityUnlinkablesReport, Refusal> {
    let strategy = load_link_unlinkables_strategy_config(
        run_artifact,
        profile_context_dirs,
        entity_link_run_strategy_hash(run_artifact),
        "run_artifact.metadata.strategy.content_hash",
    )?;
    let record_link_input_set = if strategy.record_link_input_paths.is_empty() {
        None
    } else {
        Some(
            load_record_link_inputs(RecordLinkLoadRequest {
                workspace_root: &strategy.strategy_workspace_root,
                sidecar_paths: strategy.record_link_input_paths.clone(),
                expected_profile_id: Some(loaded_profile.document.profile.clone()),
                expected_profile_digest: Some(loaded_profile.content_hash.clone()),
                expected_scope_id: None,
            })
            .map_err(|error| link_record_link_refusal(error, "link"))?,
        )
    };
    let surface_inputs =
        link_unlinkables_surface_inputs(bindings, surfaces, record_link_input_set.as_ref());
    build_entity_unlinkables_report(EntityUnlinkablesReportRequest {
        profile: &loaded_profile.document,
        support_namespace: &loaded_profile.document.patch_namespaces.aliases,
        thresholds: strategy.thresholds,
        surfaces: surface_inputs,
        record_link_feature_policies: strategy.record_link_feature_policies,
    })
}

#[derive(Debug)]
struct LinkUnlinkablesStrategyConfig {
    strategy_workspace_root: PathBuf,
    thresholds: EntityUnlinkablesThresholds,
    record_link_input_paths: Vec<PathBuf>,
    record_link_feature_policies: BTreeMap<String, RecordLinkFeaturePolicy>,
}

#[derive(Debug, Deserialize)]
struct LinkUnlinkablesStrategyDocument {
    #[serde(default)]
    solver: LinkUnlinkablesSolverConfig,
    #[serde(default)]
    match_threshold: Option<f64>,
    #[serde(default)]
    record_link: Option<LinkUnlinkablesRecordLinkSection>,
}

#[derive(Debug, Default, Deserialize)]
struct LinkUnlinkablesSolverConfig {
    #[serde(default)]
    backbone_score_min: Option<u32>,
    #[serde(default)]
    attach_score_min: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct LinkUnlinkablesRecordLinkSection {
    #[serde(default)]
    inputs: Vec<LinkUnlinkablesRecordLinkInput>,
    #[serde(default)]
    feature_policies: Vec<RecordLinkFeaturePolicy>,
}

#[derive(Debug, Deserialize)]
struct LinkUnlinkablesRecordLinkInput {
    path: PathBuf,
}

fn load_link_unlinkables_strategy_config(
    run_artifact: &EntityRunArtifact,
    context_dirs: &[PathBuf],
    expected_strategy_hash: Option<&str>,
    expected_strategy_hash_field: &'static str,
) -> Result<LinkUnlinkablesStrategyConfig, Refusal> {
    let (strategy_path, bytes) = load_link_strategy_bytes(
        run_artifact,
        context_dirs,
        expected_strategy_hash,
        expected_strategy_hash_field,
        "unlinkables diagnostics",
    )?;
    let document: LinkUnlinkablesStrategyDocument =
        serde_yaml::from_slice(&bytes).map_err(|error| {
            link_artifact_refusal(
                "Failed to parse entity link strategy for unlinkables diagnostics",
                json!({
                    "stage": "link",
                    "path": strategy_path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
    let thresholds = link_unlinkables_thresholds(&document)?;
    let (record_link_input_paths, record_link_feature_policies) =
        link_unlinkables_record_link_config(document.record_link)?;
    Ok(LinkUnlinkablesStrategyConfig {
        strategy_workspace_root: strategy_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        thresholds,
        record_link_input_paths,
        record_link_feature_policies,
    })
}

fn load_link_strategy_bytes(
    run_artifact: &EntityRunArtifact,
    context_dirs: &[PathBuf],
    expected_strategy_hash: Option<&str>,
    expected_strategy_hash_field: &'static str,
    purpose: &'static str,
) -> Result<(PathBuf, Vec<u8>), Refusal> {
    let strategy_source = run_artifact
        .summary
        .labels
        .get("strategy_source")
        .ok_or_else(|| {
            link_artifact_refusal(
                format!("Entity link run artifact does not record a strategy source for {purpose}"),
                json!({
                    "stage": "link",
                    "field": "run.summary.labels.strategy_source",
                    "writes_performed": false
                }),
            )
        })?;
    let Some(expected_strategy_hash) =
        expected_strategy_hash.filter(|strategy_hash| !strategy_hash.trim().is_empty())
    else {
        return Err(link_artifact_refusal(
            format!("Entity link run artifact does not bind a strategy hash for {purpose}"),
            json!({
                "stage": "link",
                "field": expected_strategy_hash_field,
                "writes_performed": false
            }),
        ));
    };
    let candidates = link_context_source_candidates(strategy_source, context_dirs);
    let mut mismatches = Vec::new();
    let mut load_failures = Vec::new();
    for candidate in &candidates {
        let bytes = match fs::read(candidate) {
            Ok(bytes) => bytes,
            Err(error) => {
                load_failures.push(json!({
                    "resolved_source": candidate,
                    "error": error.to_string()
                }));
                continue;
            }
        };
        let actual_hash = witness::hash_bytes(&bytes);
        if actual_hash.as_str() != expected_strategy_hash {
            mismatches.push(json!({
                "resolved_source": candidate,
                "actual": actual_hash
            }));
            continue;
        }
        return Ok((PathBuf::from(candidate), bytes));
    }
    if !mismatches.is_empty() {
        return Err(link_artifact_refusal(
            format!("Entity link strategy hash does not match {purpose} source"),
            json!({
                "stage": "link",
                "field": expected_strategy_hash_field,
                "source": strategy_source,
                "expected": expected_strategy_hash,
                "attempted_sources": candidates,
                "mismatches": mismatches,
                "load_failures": load_failures,
                "writes_performed": false
            }),
        ));
    }
    Err(link_artifact_refusal(
        format!("Failed to read entity link strategy for {purpose}"),
        json!({
            "stage": "link",
            "field": "run.summary.labels.strategy_source",
            "source": strategy_source,
            "attempted_sources": candidates,
            "load_failures": load_failures,
            "writes_performed": false
        }),
    ))
}

fn link_unlinkables_thresholds(
    document: &LinkUnlinkablesStrategyDocument,
) -> Result<EntityUnlinkablesThresholds, Refusal> {
    if let (Some(attach), Some(backbone)) = (
        document.solver.attach_score_min,
        document.solver.backbone_score_min,
    ) {
        validate_link_unlinkables_threshold_units(attach, "strategy.solver.attach_score_min")?;
        validate_link_unlinkables_threshold_units(backbone, "strategy.solver.backbone_score_min")?;
        return Ok(EntityUnlinkablesThresholds {
            threshold_source: "strategy.solver".to_string(),
            attach_score_min_units: attach,
            backbone_score_min_units: backbone,
        });
    }
    let Some(match_threshold) = document.match_threshold else {
        return Err(link_artifact_refusal(
            "Entity link strategy must declare thresholds for unlinkables diagnostics",
            json!({
                "stage": "link",
                "field": "strategy.solver|strategy.match_threshold",
                "writes_performed": false
            }),
        ));
    };
    if !match_threshold.is_finite() {
        return Err(link_artifact_refusal(
            "Entity link match threshold must be finite for unlinkables diagnostics",
            json!({
                "stage": "link",
                "field": "strategy.match_threshold",
                "actual": match_threshold,
                "writes_performed": false
            }),
        ));
    }
    let units = crate::entity::score::ScoreUnits::from_f64_ratio(match_threshold).as_u32();
    Ok(EntityUnlinkablesThresholds {
        threshold_source: "strategy.match_threshold".to_string(),
        attach_score_min_units: units,
        backbone_score_min_units: units,
    })
}

fn validate_link_unlinkables_threshold_units(
    value: u32,
    field: &'static str,
) -> Result<(), Refusal> {
    if value > crate::entity::score::ENTITY_SCORE_SCALE {
        return Err(link_artifact_refusal(
            "Entity link unlinkables threshold exceeds the score scale",
            json!({
                "stage": "link",
                "field": field,
                "actual": value,
                "max": crate::entity::score::ENTITY_SCORE_SCALE,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn link_unlinkables_record_link_config(
    section: Option<LinkUnlinkablesRecordLinkSection>,
) -> Result<(Vec<PathBuf>, BTreeMap<String, RecordLinkFeaturePolicy>), Refusal> {
    let Some(section) = section else {
        return Ok((Vec::new(), BTreeMap::new()));
    };
    let mut input_paths = Vec::with_capacity(section.inputs.len());
    for input in section.inputs {
        input_paths.push(validate_link_unlinkables_record_link_path(input.path)?);
    }
    let mut feature_policies = BTreeMap::new();
    for policy in section.feature_policies {
        if feature_policies
            .insert(policy.feature_id.clone(), policy)
            .is_some()
        {
            return Err(link_artifact_refusal(
                "Record-link feature policies must be unique for unlinkables diagnostics",
                json!({
                    "stage": "link",
                    "field": "record_link.feature_policies",
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok((input_paths, feature_policies))
}

fn validate_link_unlinkables_record_link_path(path: PathBuf) -> Result<PathBuf, Refusal> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(link_artifact_refusal(
            "Record-link sidecar paths must be strategy-relative safe paths for unlinkables diagnostics",
            json!({
                "stage": "link",
                "field": "record_link.inputs.path",
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    Ok(path)
}

fn link_unlinkables_surface_inputs(
    bindings: &[EntityLinkObservationSurfaceBinding],
    surfaces: &[PreparedSurfaceRecord],
    record_link_input_set: Option<&RecordLinkInputSet>,
) -> Vec<EntityUnlinkablesSurfaceInput> {
    let surface_by_id = surfaces
        .iter()
        .map(|surface| (surface.surface_id.as_str(), surface))
        .collect::<BTreeMap<_, _>>();
    let mut binding_keys =
        BTreeMap::<(EntityUnlinkablesSurfaceSide, String), LinkSurfaceBinding>::new();
    for binding in bindings {
        let key = (
            unlinkables_surface_side(binding.side),
            binding.surface_id.clone(),
        );
        let entry = binding_keys.entry(key).or_default();
        entry.link_ids.insert(binding.link_id.clone());
        if let Some(source_row_id) = binding
            .source_row_id
            .as_ref()
            .filter(|source_row_id| !source_row_id.trim().is_empty())
        {
            entry.source_row_ids.insert(source_row_id.clone());
        }
    }
    let feature_bags_by_row_id = record_link_input_set
        .map(record_link_feature_bags_by_row_id)
        .unwrap_or_default();

    let mut inputs = Vec::new();
    let mut assigned_surface_ids = BTreeSet::<String>::new();
    for ((side, surface_id), binding) in binding_keys {
        let Some(surface) = surface_by_id.get(surface_id.as_str()) else {
            continue;
        };
        assigned_surface_ids.insert(surface_id.clone());
        let feature_bag =
            merged_record_link_feature_bag(&binding.source_row_ids, &feature_bags_by_row_id);
        inputs.push(EntityUnlinkablesSurfaceInput {
            side,
            surface: (*surface).clone(),
            link_ids: binding.link_ids.into_iter().collect(),
            record_link_features: feature_bag.values,
            quarantined_record_link_features: feature_bag.quarantined,
        });
    }
    for surface in surfaces {
        if assigned_surface_ids.contains(&surface.surface_id) {
            continue;
        }
        inputs.push(EntityUnlinkablesSurfaceInput {
            side: EntityUnlinkablesSurfaceSide::Unassigned,
            surface: surface.clone(),
            link_ids: Vec::new(),
            record_link_features: BTreeMap::new(),
            quarantined_record_link_features: BTreeMap::new(),
        });
    }
    inputs.sort_by(|left, right| {
        left.side
            .cmp(&right.side)
            .then_with(|| left.surface.surface_id.cmp(&right.surface.surface_id))
            .then_with(|| left.link_ids.cmp(&right.link_ids))
    });
    inputs
}

#[derive(Debug, Default)]
struct LinkSurfaceBinding {
    link_ids: BTreeSet<String>,
    source_row_ids: BTreeSet<String>,
}

#[derive(Debug, Default, Clone)]
struct LinkRecordFeatureBag {
    values: BTreeMap<String, RecordLinkFeatureValue>,
    quarantined: BTreeMap<String, String>,
}

fn record_link_feature_bags_by_row_id(
    input_set: &RecordLinkInputSet,
) -> BTreeMap<String, LinkRecordFeatureBag> {
    let mut by_row_id = BTreeMap::<String, LinkRecordFeatureBag>::new();
    for input in &input_set.inputs {
        for record in &input.sidecar.records {
            let bag = record_link_feature_bag(record);
            for row_key in record_link_row_keys(record) {
                merge_record_link_feature_bag(by_row_id.entry(row_key).or_default(), &bag);
            }
        }
    }
    by_row_id
}

fn record_link_feature_bag(record: &RecordLinkInputRecord) -> LinkRecordFeatureBag {
    let mut bag = LinkRecordFeatureBag::default();
    for view in &record.comparison_views {
        let (feature_id, value) = record_link_feature_value(view);
        insert_record_link_feature_value(&mut bag, feature_id, value);
    }
    for field in &record.quarantined_fields {
        insert_record_link_quarantine(&mut bag, field);
    }
    bag
}

fn merged_record_link_feature_bag(
    row_ids: &BTreeSet<String>,
    by_row_id: &BTreeMap<String, LinkRecordFeatureBag>,
) -> LinkRecordFeatureBag {
    let mut merged = LinkRecordFeatureBag::default();
    for row_id in row_ids {
        if let Some(bag) = by_row_id.get(row_id) {
            merge_record_link_feature_bag(&mut merged, bag);
        }
    }
    merged
}

fn merge_record_link_feature_bag(target: &mut LinkRecordFeatureBag, source: &LinkRecordFeatureBag) {
    for (feature_id, value) in &source.values {
        insert_record_link_feature_value(target, feature_id.clone(), value.clone());
    }
    for (feature_id, reason) in &source.quarantined {
        target
            .quarantined
            .entry(feature_id.clone())
            .or_insert_with(|| reason.clone());
    }
}

fn insert_record_link_feature_value(
    bag: &mut LinkRecordFeatureBag,
    feature_id: String,
    value: RecordLinkFeatureValue,
) {
    if let Some(existing) = bag.values.get(&feature_id) {
        if existing != &value {
            bag.values.remove(&feature_id);
            bag.quarantined
                .insert(feature_id, "record_link_feature_conflict".to_string());
        }
        return;
    }
    if !bag.quarantined.contains_key(&feature_id) {
        bag.values.insert(feature_id, value);
    }
}

fn insert_record_link_quarantine(
    bag: &mut LinkRecordFeatureBag,
    field: &RecordLinkQuarantinedField,
) {
    bag.values.remove(&field.feature_id);
    bag.quarantined.insert(
        field.feature_id.clone(),
        record_link_field_disposition_reason(field.reason).to_string(),
    );
}

fn record_link_feature_value(view: &RecordLinkComparisonView) -> (String, RecordLinkFeatureValue) {
    match view {
        RecordLinkComparisonView::Numeric {
            feature_id,
            units,
            scaled_value,
            scale,
            ..
        } => (
            feature_id.clone(),
            RecordLinkFeatureValue::Numeric {
                units: units.clone(),
                scaled_value: *scaled_value,
                scale: *scale,
            },
        ),
        RecordLinkComparisonView::Date {
            feature_id, value, ..
        } => (
            feature_id.clone(),
            RecordLinkFeatureValue::Date {
                value: value.clone(),
            },
        ),
        RecordLinkComparisonView::Categorical {
            feature_id, value, ..
        } => (
            feature_id.clone(),
            RecordLinkFeatureValue::Categorical {
                value: value.clone(),
            },
        ),
    }
}

fn record_link_row_keys(record: &RecordLinkInputRecord) -> BTreeSet<String> {
    [
        record.source_ref.source_object_id.as_str(),
        record.source_ref.source_locator.locator.as_str(),
        record.record_id.as_str(),
        record.subject_observation_ref.observation_id.as_str(),
    ]
    .into_iter()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(ToOwned::to_owned)
    .collect()
}

fn record_link_field_disposition_reason(reason: RecordLinkFieldDispositionReason) -> &'static str {
    match reason {
        RecordLinkFieldDispositionReason::MissingField => "missing_field",
        RecordLinkFieldDispositionReason::MalformedField => "malformed_field",
        RecordLinkFieldDispositionReason::Overflow => "overflow",
        RecordLinkFieldDispositionReason::IncomparableField => "incomparable_field",
        RecordLinkFieldDispositionReason::DuplicateRecordId => "duplicate_record_id",
    }
}

fn unlinkables_surface_side(role: EntityLinkRole) -> EntityUnlinkablesSurfaceSide {
    match role {
        EntityLinkRole::Reference => EntityUnlinkablesSurfaceSide::Reference,
        EntityLinkRole::Target => EntityUnlinkablesSurfaceSide::Target,
    }
}

fn link_record_link_refusal(
    error: crate::entity::record_link::RecordLinkCoreError,
    stage: &'static str,
) -> Refusal {
    let kind = match error.code {
        crate::entity::record_link::RecordLinkCoreErrorCode::Io => EntityRefusalKind::IoBudget,
        crate::entity::record_link::RecordLinkCoreErrorCode::Budget => {
            EntityRefusalKind::CandidateBudget
        }
        crate::entity::record_link::RecordLinkCoreErrorCode::ArtifactContract
        | crate::entity::record_link::RecordLinkCoreErrorCode::Path => {
            EntityRefusalKind::ArtifactContract
        }
    };
    kind.to_refusal(
        error.message,
        json!({
            "stage": stage,
            "record_link_stage": error.stage,
            "reason": error.reason,
            "writes_performed": false
        }),
        Some("Use canon entity link to regenerate link/link.json".to_string()),
    )
}

fn link_publication_parent_generation_id(artifact: &EntityLinkArtifact) -> Result<String, Refusal> {
    let mut matches = artifact
        .metadata
        .upstream_artifacts
        .iter()
        .filter(|reference| reference.version == CANON_ENTITY_STAGE_PUBLICATION_VERSION);
    let Some(reference) = matches.next() else {
        return Err(link_artifact_refusal(
            "Entity link artifact is missing its committed run publication parent",
            json!({
                "stage": "link",
                "field": "metadata.upstream_artifacts",
                "expected_version": CANON_ENTITY_STAGE_PUBLICATION_VERSION,
                "writes_performed": false
            }),
        ));
    };
    if matches.next().is_some() {
        return Err(link_artifact_refusal(
            "Entity link artifact must name exactly one committed run publication parent",
            json!({
                "stage": "link",
                "field": "metadata.upstream_artifacts",
                "expected_version": CANON_ENTITY_STAGE_PUBLICATION_VERSION,
                "writes_performed": false
            }),
        ));
    }
    if reference.content_hash.trim().is_empty() {
        return Err(link_artifact_refusal(
            "Entity link committed run publication parent must carry a generation id",
            json!({
                "stage": "link",
                "field": "metadata.upstream_artifacts.content_hash",
                "version": CANON_ENTITY_STAGE_PUBLICATION_VERSION,
                "writes_performed": false
            }),
        ));
    }
    Ok(reference.content_hash.clone())
}

pub fn validate_entity_link_artifact_contract(
    artifact: &EntityLinkArtifact,
) -> Result<(), Refusal> {
    if artifact.version != ENTITY_LINK_VERSION {
        return Err(link_artifact_refusal(
            "Entity link artifact has the wrong contract version",
            json!({
                "stage": "link",
                "field": "version",
                "expected": ENTITY_LINK_VERSION,
                "actual": artifact.version,
                "writes_performed": false
            }),
        ));
    }
    if artifact.artifact_content_hash.trim().is_empty() {
        return Err(link_artifact_refusal(
            "Entity link artifact must carry a content hash",
            json!({
                "stage": "link",
                "field": "artifact_content_hash",
                "writes_performed": false
            }),
        ));
    }
    if artifact.metadata.artifact_content_hash != artifact.artifact_content_hash {
        return Err(link_artifact_refusal(
            "Entity link artifact metadata hash does not match artifact hash",
            json!({
                "stage": "link",
                "field": "metadata.artifact_content_hash",
                "expected": artifact.artifact_content_hash,
                "actual": artifact.metadata.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    validate_link_upstreams(artifact)?;
    validate_link_assignment_alignment_artifacts(artifact)?;
    validate_link_profile_source_reference(artifact)?;
    validate_safe_relative_path(&artifact.materialized_rows_path, "materialized_rows_path")?;
    validate_safe_relative_path(
        &artifact.observation_surface_bindings_path,
        "observation_surface_bindings_path",
    )?;
    if artifact
        .observation_surface_bindings_content_hash
        .trim()
        .is_empty()
    {
        return Err(link_artifact_refusal(
            "Entity link artifact must bind observation/surface bindings",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings_content_hash",
                "writes_performed": false
            }),
        ));
    }
    validate_link_summary(artifact)?;
    validate_link_decision_artifact(&artifact.decision_artifact)?;
    if let Some(unlinkables) = &artifact.unlinkables {
        validate_entity_unlinkables_report(unlinkables)?;
    }
    if artifact.decision_artifact.summary != artifact.summary {
        return Err(link_artifact_refusal(
            "Entity link summary must match the nested decision summary",
            json!({
                "stage": "link",
                "field": "summary",
                "writes_performed": false
            }),
        ));
    }
    let expected = hash_link_artifact_without_self(artifact)?;
    if artifact.artifact_content_hash != expected {
        return Err(link_artifact_refusal(
            "Entity link artifact content hash does not match its payload",
            json!({
                "stage": "link",
                "field": "artifact_content_hash",
                "expected": expected,
                "actual": artifact.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

pub fn validate_entity_link_artifact_raw_shape(value: &Value) -> Result<(), Refusal> {
    validate_known_object_keys(
        value,
        &[
            "version",
            "artifact_content_hash",
            "metadata",
            "summary",
            "mode",
            "reference",
            "target",
            "materialized_rows_path",
            "materialized_rows_content_hash",
            "profile_source",
            "observation_surface_bindings_path",
            "observation_surface_bindings_content_hash",
            "shared_run_artifact",
            "shared_solve_artifact",
            "assignment_alignment_artifacts",
            "decision_artifact",
            "unlinkables",
            "next_commands",
        ],
        "",
    )?;
    if let Some(decision_artifact) = value.get("decision_artifact") {
        validate_known_object_keys(
            decision_artifact,
            &[
                "version",
                "artifact_content_hash",
                "strategy",
                "registry",
                "reference_tape",
                "target_tape",
                "summary",
                "matches",
                "unmatched",
                "ambiguous",
                "conflict_warnings",
                "gold_score",
                "write_back",
            ],
            "decision_artifact.",
        )?;
    }
    if let Some(profile_source) = value.get("profile_source") {
        validate_known_object_keys(
            profile_source,
            &["source", "content_hash"],
            "profile_source.",
        )?;
    }
    Ok(())
}

pub fn validate_entity_link_artifact_at_path(
    artifact: &EntityLinkArtifact,
    artifact_path: &Path,
) -> Result<(), Refusal> {
    validate_entity_link_artifact_contract(artifact)?;
    let base_dir = entity_link_artifact_base_dir(artifact_path);
    let materialized_path = base_dir.join(&artifact.materialized_rows_path);
    let work_dir = entity_link_work_dir_from_artifact_path(artifact_path);
    let materialized_bytes =
        read_link_committed_or_stable_bytes(&work_dir, &materialized_path, "materialized rows")?;
    let actual_hash = witness::hash_bytes(&materialized_bytes);
    if actual_hash != artifact.materialized_rows_content_hash {
        return Err(link_artifact_refusal(
            "Entity link materialized rows hash does not match the linked payload",
            json!({
                "stage": "link",
                "field": "materialized_rows_content_hash",
                "path": materialized_path.display().to_string(),
                "expected": artifact.materialized_rows_content_hash,
                "actual": actual_hash,
                "writes_performed": false
            }),
        ));
    }
    validate_link_profile_source_at_path(artifact, artifact_path)?;
    read_validated_entity_link_observation_surface_bindings_at_path(artifact, artifact_path)?;
    validate_link_assignment_alignment_artifacts_at_path(artifact, artifact_path)?;
    Ok(())
}

pub fn read_validated_entity_link_observation_surface_bindings_at_path(
    artifact: &EntityLinkArtifact,
    artifact_path: &Path,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    validate_entity_link_artifact_contract(artifact)?;
    let base_dir = entity_link_artifact_base_dir(artifact_path);
    let bindings_path = base_dir.join(&artifact.observation_surface_bindings_path);
    let work_dir = entity_link_work_dir_from_artifact_path(artifact_path);
    let bindings_bytes = read_link_committed_or_stable_bytes(
        &work_dir,
        &bindings_path,
        "observation/surface bindings",
    )?;
    let actual_hash = witness::hash_bytes(&bindings_bytes);
    if actual_hash != artifact.observation_surface_bindings_content_hash {
        return Err(link_artifact_refusal(
            "Entity link observation/surface bindings hash does not match the linked payload",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings_content_hash",
                "path": bindings_path.display().to_string(),
                "expected": artifact.observation_surface_bindings_content_hash,
                "actual": actual_hash,
                "writes_performed": false
            }),
        ));
    }
    let bindings = read_observation_surface_bindings_bytes(&bindings_bytes, &bindings_path)?;
    validate_entity_link_observation_surface_bindings(artifact, &bindings)?;
    Ok(bindings)
}

pub fn read_derivation_validated_entity_link_observation_surface_bindings_at_path(
    artifact: &EntityLinkArtifact,
    artifact_path: &Path,
    run_artifact: &EntityRunArtifact,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    validate_entity_link_artifact_contract(artifact)?;
    let base_dir = entity_link_artifact_base_dir(artifact_path);
    let materialized_path = base_dir.join(&artifact.materialized_rows_path);
    let work_dir = entity_link_work_dir_from_artifact_path(artifact_path);
    let materialized_bytes =
        read_link_committed_or_stable_bytes(&work_dir, &materialized_path, "materialized rows")?;
    let materialized_hash = witness::hash_bytes(&materialized_bytes);
    if materialized_hash != artifact.materialized_rows_content_hash {
        return Err(link_artifact_refusal(
            "Entity link materialized rows hash does not match the linked payload",
            json!({
                "stage": "link",
                "field": "materialized_rows_content_hash",
                "path": materialized_path.display().to_string(),
                "expected": artifact.materialized_rows_content_hash,
                "actual": materialized_hash,
                "writes_performed": false
            }),
        ));
    }
    validate_entity_link_run_continuity(artifact, run_artifact, &materialized_hash)?;
    let actual =
        read_validated_entity_link_observation_surface_bindings_at_path(artifact, artifact_path)?;
    let profile_context_dirs = link_profile_source_context_dirs(artifact_path);
    let expected = build_link_observation_surface_bindings_from_materialized_bytes(
        &materialized_bytes,
        LinkObservationSurfaceBindingBuildContext {
            work_dir: &work_dir,
            materialized_path: &materialized_path,
            artifact,
            profile_context_dirs: &profile_context_dirs,
            run_artifact,
            expected_strategy_hash: entity_link_run_strategy_hash(run_artifact),
            expected_strategy_hash_field: "run_artifact.metadata.strategy.content_hash",
        },
    )?;
    validate_entity_link_observation_surface_bindings(artifact, &expected)?;
    if actual != expected {
        let first_mismatch_index = actual
            .iter()
            .zip(expected.iter())
            .position(|(actual, expected)| actual != expected);
        return Err(link_artifact_refusal(
            "Entity link observation/surface bindings do not match deterministic derivation",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "reason": "derivation_mismatch",
                "actual_records": actual.len(),
                "expected_records": expected.len(),
                "first_mismatch_index": first_mismatch_index,
                "writes_performed": false
            }),
        ));
    }
    Ok(actual)
}

pub fn validate_entity_link_observation_surface_bindings(
    artifact: &EntityLinkArtifact,
    bindings: &[EntityLinkObservationSurfaceBinding],
) -> Result<(), Refusal> {
    let mut reference_ids = BTreeSet::new();
    let mut target_ids = BTreeSet::new();
    let mut reference_ordinals = BTreeSet::new();
    let mut target_ordinals = BTreeSet::new();

    for binding in bindings {
        validate_observation_surface_binding_record(binding)?;
        let (ids, ordinals) = match binding.side {
            EntityLinkRole::Reference => (&mut reference_ids, &mut reference_ordinals),
            EntityLinkRole::Target => (&mut target_ids, &mut target_ordinals),
        };
        if !ids.insert(binding.link_id.as_str()) {
            return Err(link_artifact_refusal(
                "Entity link observation/surface bindings contain a duplicate link id",
                json!({
                    "stage": "link",
                    "field": "observation_surface_bindings",
                    "side": binding.side.as_str(),
                    "link_id": binding.link_id,
                    "writes_performed": false
                }),
            ));
        }
        if !ordinals.insert(binding.source_ordinal) {
            return Err(link_artifact_refusal(
                "Entity link observation/surface bindings contain a duplicate source ordinal",
                json!({
                    "stage": "link",
                    "field": "observation_surface_bindings",
                    "side": binding.side.as_str(),
                    "source_ordinal": binding.source_ordinal,
                    "writes_performed": false
                }),
            ));
        }
    }

    if reference_ids.len() != artifact.reference.row_count as usize
        || target_ids.len() != artifact.target.row_count as usize
    {
        return Err(link_artifact_refusal(
            "Entity link observation/surface binding counts do not match link input counts",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "expected_reference": artifact.reference.row_count,
                "actual_reference": reference_ids.len(),
                "expected_target": artifact.target.row_count,
                "actual_target": target_ids.len(),
                "writes_performed": false
            }),
        ));
    }

    if bindings.windows(2).any(|pair| {
        matches!(
            observation_surface_binding_cmp(&pair[0], &pair[1]),
            std::cmp::Ordering::Greater
        )
    }) {
        return Err(link_artifact_refusal(
            "Entity link observation/surface bindings must be sorted deterministically",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "reason": "nondeterministic_order",
                "writes_performed": false
            }),
        ));
    }

    let decision_targets = decision_target_ids(&artifact.decision_artifact)?;
    if decision_targets != target_ids {
        return Err(link_artifact_refusal(
            "Entity link observation/surface bindings do not cover classified target ids",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "expected_target_ids": decision_targets,
                "actual_target_ids": target_ids,
                "writes_performed": false
            }),
        ));
    }

    let decision_references = decision_reference_ids(&artifact.decision_artifact);
    let missing_references = decision_references
        .difference(&reference_ids)
        .copied()
        .collect::<Vec<_>>();
    if !missing_references.is_empty() {
        return Err(link_artifact_refusal(
            "Entity link observation/surface bindings do not cover decision reference ids",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "missing_reference_ids": missing_references,
                "writes_performed": false
            }),
        ));
    }

    Ok(())
}

fn entity_link_artifact_base_dir(artifact_path: &Path) -> &Path {
    artifact_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn validate_entity_link_run_continuity(
    artifact: &EntityLinkArtifact,
    run_artifact: &EntityRunArtifact,
    materialized_hash: &str,
) -> Result<(), Refusal> {
    if run_artifact.metadata.artifact_content_hash != run_artifact.artifact_content_hash {
        return Err(link_artifact_refusal(
            "Entity run artifact metadata hash does not match artifact hash",
            json!({
                "stage": "link",
                "field": "run_artifact.metadata.artifact_content_hash",
                "expected": run_artifact.artifact_content_hash,
                "actual": run_artifact.metadata.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    // Persisted v1 run artifacts must be raw-validated by callers before typed
    // derivation replay. `EntityRunArtifact` intentionally omits raw v1 schema
    // metadata, so this typed continuity check preserves only fields available
    // after deserialization.
    let expected_run = EntityArtifactReference {
        version: run_artifact.version.clone(),
        content_hash: run_artifact.artifact_content_hash.clone(),
    };
    if artifact.shared_run_artifact != expected_run {
        return Err(link_artifact_refusal(
            "Entity link artifact does not reference the supplied run artifact",
            json!({
                "stage": "link",
                "field": "shared_run_artifact",
                "expected": expected_run,
                "actual": artifact.shared_run_artifact,
                "writes_performed": false
            }),
        ));
    }
    let expected_solve = solve_stage_reference(run_artifact)?;
    if artifact.shared_solve_artifact != expected_solve {
        return Err(link_artifact_refusal(
            "Entity link artifact does not reference the supplied run artifact solve stage",
            json!({
                "stage": "link",
                "field": "shared_solve_artifact",
                "expected": expected_solve,
                "actual": artifact.shared_solve_artifact,
                "writes_performed": false
            }),
        ));
    }
    let Some(input) = run_artifact.metadata.input.as_ref() else {
        return Err(link_artifact_refusal(
            "Entity run artifact must bind the materialized link input",
            json!({
                "stage": "link",
                "field": "run_artifact.metadata.input",
                "writes_performed": false
            }),
        ));
    };
    let expected_rows = artifact
        .reference
        .row_count
        .checked_add(artifact.target.row_count)
        .ok_or_else(|| {
            link_artifact_refusal(
                "Entity link input row counts overflowed",
                json!({
                    "stage": "link",
                    "field": "input.row_count",
                    "writes_performed": false
                }),
            )
        })?;
    if input.row_count != expected_rows {
        return Err(link_artifact_refusal(
            "Entity run artifact input row count does not match link input rows",
            json!({
                "stage": "link",
                "field": "run_artifact.metadata.input.row_count",
                "expected": expected_rows,
                "actual": input.row_count,
                "writes_performed": false
            }),
        ));
    }
    if input.content_hash != materialized_hash {
        return Err(link_artifact_refusal(
            "Entity run artifact input hash does not match link materialized rows",
            json!({
                "stage": "link",
                "field": "run_artifact.metadata.input.content_hash",
                "expected": input.content_hash,
                "actual": materialized_hash,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn entity_link_run_strategy_hash(run_artifact: &EntityRunArtifact) -> Option<&str> {
    let strategy_hash = run_artifact.metadata.strategy.content_hash.as_str();
    if strategy_hash.trim().is_empty() {
        None
    } else {
        Some(strategy_hash)
    }
}

fn link_profile_source_from_request(
    profile: &str,
    run_artifact: &EntityRunArtifact,
) -> Result<EntityLinkProfileSource, Refusal> {
    let loaded_profile = load_link_profile_source(profile)?;
    let source = EntityLinkProfileSource {
        source: profile.to_string(),
        content_hash: loaded_profile.content_hash.clone(),
    };
    validate_loaded_link_profile_against_run(&source, &loaded_profile, run_artifact)?;
    Ok(source)
}

fn validate_link_profile_source_reference(artifact: &EntityLinkArtifact) -> Result<(), Refusal> {
    if artifact.profile_source.source.trim().is_empty() {
        return Err(link_artifact_refusal(
            "Entity link artifact must bind the prepare profile source",
            json!({
                "stage": "link",
                "field": "profile_source.source",
                "writes_performed": false
            }),
        ));
    }
    if artifact.profile_source.content_hash.trim().is_empty() {
        return Err(link_artifact_refusal(
            "Entity link artifact must bind the prepare profile source hash",
            json!({
                "stage": "link",
                "field": "profile_source.content_hash",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_link_profile_source_at_path(
    artifact: &EntityLinkArtifact,
    artifact_path: &Path,
) -> Result<LoadedPrepareProfile, Refusal> {
    validate_link_profile_source_reference(artifact)?;
    let context_dirs = link_profile_source_context_dirs(artifact_path);
    load_and_validate_link_profile_source(&artifact.profile_source, &context_dirs)
}

fn load_and_validate_link_profile_source(
    profile_source: &EntityLinkProfileSource,
    context_dirs: &[PathBuf],
) -> Result<LoadedPrepareProfile, Refusal> {
    let candidates = link_profile_source_candidates(&profile_source.source, context_dirs);
    let mut mismatches = Vec::new();
    let mut load_failures = Vec::new();
    for candidate in &candidates {
        match load_prepare_profile_with_hash(candidate) {
            Ok(loaded_profile) if loaded_profile.content_hash == profile_source.content_hash => {
                return Ok(loaded_profile);
            }
            Ok(loaded_profile) => {
                mismatches.push(json!({
                    "resolved_source": candidate,
                    "actual": loaded_profile.content_hash
                }));
            }
            Err(refusal) => {
                load_failures.push(json!({
                    "resolved_source": candidate,
                    "refusal": refusal
                }));
            }
        }
    }
    if !mismatches.is_empty() {
        return Err(link_artifact_refusal(
            "Entity link profile source hash does not match the linked payload",
            json!({
                "stage": "link",
                "field": "profile_source.content_hash",
                "source": profile_source.source,
                "expected": profile_source.content_hash,
                "attempted_sources": candidates,
                "mismatches": mismatches,
                "writes_performed": false
            }),
        ));
    }
    Err(link_artifact_refusal(
        "Failed to load entity link prepare profile source",
        json!({
            "stage": "link",
            "field": "profile_source.source",
            "source": profile_source.source,
            "attempted_sources": candidates,
            "load_failures": load_failures,
            "writes_performed": false
        }),
    ))
}

fn validate_link_profile_source_against_run(
    profile_source: &EntityLinkProfileSource,
    context_dirs: &[PathBuf],
    run_artifact: &EntityRunArtifact,
) -> Result<LoadedPrepareProfile, Refusal> {
    let loaded_profile = load_and_validate_link_profile_source(profile_source, context_dirs)?;
    validate_loaded_link_profile_against_run(profile_source, &loaded_profile, run_artifact)?;
    Ok(loaded_profile)
}

fn load_link_profile_source(profile: &str) -> Result<LoadedPrepareProfile, Refusal> {
    load_prepare_profile_with_hash(profile).map_err(|refusal| {
        link_artifact_refusal(
            "Failed to load entity link prepare profile source",
            json!({
                "stage": "link",
                "field": "profile_source.source",
                "source": profile,
                "refusal": refusal,
                "writes_performed": false
            }),
        )
    })
}

fn link_profile_source_context_dirs(artifact_path: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut current = Some(entity_link_artifact_base_dir(artifact_path));
    while let Some(dir) = current {
        let candidate = dir.to_path_buf();
        if !dirs.contains(&candidate) {
            dirs.push(candidate);
        }
        current = dir.parent().filter(|parent| !parent.as_os_str().is_empty());
    }
    dirs
}

fn link_profile_source_candidates(profile: &str, context_dirs: &[PathBuf]) -> Vec<String> {
    link_context_source_candidates(profile, context_dirs)
}

fn link_context_source_candidates(source_label: &str, context_dirs: &[PathBuf]) -> Vec<String> {
    let source = Path::new(source_label);
    let mut candidates = vec![source_label.to_string()];
    if source.is_absolute() || !link_source_is_path_like(source_label) {
        return candidates;
    }
    for dir in context_dirs {
        let candidate = dir.join(source).to_string_lossy().into_owned();
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

fn link_source_is_path_like(source_label: &str) -> bool {
    let source = Path::new(source_label);
    source.components().count() > 1 || source.extension().is_some()
}

fn validate_loaded_link_profile_against_run(
    profile_source: &EntityLinkProfileSource,
    loaded_profile: &LoadedPrepareProfile,
    run_artifact: &EntityRunArtifact,
) -> Result<(), Refusal> {
    if loaded_profile.content_hash != profile_source.content_hash {
        return Err(link_artifact_refusal(
            "Entity link profile source hash does not match the loaded profile",
            json!({
                "stage": "link",
                "field": "profile_source.content_hash",
                "source": profile_source.source,
                "expected": profile_source.content_hash,
                "actual": loaded_profile.content_hash,
                "writes_performed": false
            }),
        ));
    }
    let mut actual_profile = loaded_profile.document.to_reference();
    actual_profile.content_hash = Some(loaded_profile.content_hash.clone());
    if actual_profile != run_artifact.metadata.profile {
        return Err(link_artifact_refusal(
            "Entity link profile source does not match the run artifact profile",
            json!({
                "stage": "link",
                "field": "run_artifact.metadata.profile",
                "expected": run_artifact.metadata.profile,
                "actual": actual_profile,
                "writes_performed": false
            }),
        ));
    }
    let firewall = &run_artifact.orchestration.profile_firewall;
    if firewall.profile_id != loaded_profile.document.profile
        || firewall.profile_version != loaded_profile.document.version
        || firewall.identity_semantics != loaded_profile.document.identity_semantics
        || firewall.canonical_type != loaded_profile.document.canonical_type
    {
        return Err(link_artifact_refusal(
            "Entity link profile source does not match the run profile firewall",
            json!({
                "stage": "link",
                "field": "run_artifact.orchestration.profile_firewall",
                "expected": {
                    "profile_id": firewall.profile_id,
                    "profile_version": firewall.profile_version,
                    "identity_semantics": firewall.identity_semantics,
                    "canonical_type": firewall.canonical_type
                },
                "actual": {
                    "profile_id": loaded_profile.document.profile,
                    "profile_version": loaded_profile.document.version,
                    "identity_semantics": loaded_profile.document.identity_semantics,
                    "canonical_type": loaded_profile.document.canonical_type
                },
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn prepare_contract_for_link_profile(
    loaded_profile: &LoadedPrepareProfile,
) -> Result<PrepareInputContract, Refusal> {
    let mut contract = if let Some(mapping) = loaded_profile.prepare_mapping.clone() {
        PrepareInputContract::new(&loaded_profile.document, mapping)
    } else {
        PrepareInputContract::for_builtin_profile(&loaded_profile.document)
    }
    .map_err(|refusal| {
        link_artifact_refusal(
            "Failed to build entity link prepare contract for observation/surface bindings",
            json!({
                "stage": "link",
                "profile_id": loaded_profile.document.profile,
                "refusal": refusal,
                "writes_performed": false
            }),
        )
    })?;
    contract.profile.content_hash = Some(loaded_profile.content_hash.clone());
    Ok(contract)
}

fn read_link_run_surfaces(
    work_dir: &Path,
    run_artifact: &EntityRunArtifact,
    loaded_profile: &LoadedPrepareProfile,
) -> Result<Vec<PreparedSurfaceRecord>, Refusal> {
    validate_safe_relative_path(
        &run_artifact.work_dir.surfaces_path,
        "run_artifact.work_dir.surfaces_path",
    )?;
    let surfaces_path = work_dir.join(&run_artifact.work_dir.surfaces_path);
    let file = File::open(&surfaces_path).map_err(|error| {
        link_io_refusal(
            "Failed to read entity link run-produced surfaces",
            &surfaces_path,
            error,
        )
    })?;
    let reader = BufReader::new(file);
    let mut surfaces = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| {
            link_io_refusal(
                "Failed to read entity link run-produced surface row",
                &surfaces_path,
                error,
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let surface: PreparedSurfaceRecord = serde_json::from_str(&line).map_err(|error| {
            link_artifact_refusal(
                "Failed to parse entity link run-produced surface row",
                json!({
                    "stage": "link",
                    "field": "run_artifact.work_dir.surfaces_path",
                    "path": surfaces_path.display().to_string(),
                    "line_number": index + 1,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        if surface.profile_id != loaded_profile.document.profile {
            return Err(link_artifact_refusal(
                "Entity link run-produced surface profile does not match profile source",
                json!({
                    "stage": "link",
                    "field": "run_artifact.work_dir.surfaces_path",
                    "path": surfaces_path.display().to_string(),
                    "surface_id": surface.surface_id,
                    "expected": loaded_profile.document.profile,
                    "actual": surface.profile_id,
                    "writes_performed": false
                }),
            ));
        }
        surfaces.push(surface);
    }
    if surfaces.is_empty() {
        return Err(link_artifact_refusal(
            "Entity link run artifact must provide prepared surfaces",
            json!({
                "stage": "link",
                "field": "run_artifact.work_dir.surfaces_path",
                "path": surfaces_path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    Ok(surfaces)
}

fn validate_observation_surface_binding_record(
    binding: &EntityLinkObservationSurfaceBinding,
) -> Result<(), Refusal> {
    let mut missing = Vec::new();
    if binding.version != ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION {
        return Err(link_artifact_refusal(
            "Entity link observation/surface binding has the wrong version",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings.version",
                "expected": ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION,
                "actual": binding.version,
                "writes_performed": false
            }),
        ));
    }
    if binding.link_id.trim().is_empty() {
        missing.push("link_id");
    }
    if binding.source_ordinal == 0 {
        missing.push("source_ordinal");
    }
    if binding.surface_id.trim().is_empty() {
        missing.push("surface_id");
    }
    if binding.profile_id.trim().is_empty() {
        missing.push("profile_id");
    }
    if binding.surface_binding_hash.trim().is_empty() {
        missing.push("surface_binding_hash");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(link_artifact_refusal(
            "Entity link observation/surface binding is missing required fields",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "missing": missing,
                "writes_performed": false
            }),
        ))
    }
}

fn validate_known_object_keys(
    value: &Value,
    allowed: &[&str],
    field_prefix: &str,
) -> Result<(), Refusal> {
    let Some(object) = value.as_object() else {
        let field = field_prefix.trim_end_matches('.');
        return Err(link_artifact_refusal(
            "Entity link artifact has invalid object shape",
            json!({
                "stage": "link",
                "field": if field.is_empty() { "artifact" } else { field },
                "writes_performed": false
            }),
        ));
    };
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(link_artifact_refusal(
                "Entity link artifact contains an unknown field",
                json!({
                    "stage": "link",
                    "field": format!("{field_prefix}{key}"),
                    "unknown_field": key,
                    "allowed_fields": allowed,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MaterializedLinkRows {
    reference_rows: u64,
    target_rows: u64,
}

fn materialize_directional_rows(
    reference_rows: &Path,
    target_rows: &Path,
    output: &Path,
) -> Result<MaterializedLinkRows, Refusal> {
    let reference = load_link_input_rows(reference_rows, EntityLinkRole::Reference)?;
    let target = load_link_input_rows(target_rows, EntityLinkRole::Target)?;

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            link_io_refusal(
                "Failed to create entity link materialization directory",
                parent,
                error,
            )
        })?;
    }

    let merged_headers = merged_headers(&reference.headers, &target.headers);
    let file = File::create(output).map_err(|error| {
        link_io_refusal(
            "Failed to create entity link materialized rows",
            output,
            error,
        )
    })?;
    let mut writer = WriterBuilder::new().from_writer(file);
    let mut output_headers = merged_headers.clone();
    output_headers.push(LINK_SIDE_COLUMN.to_string());
    output_headers.push(LINK_SOURCE_NAME_COLUMN.to_string());
    output_headers.push(LINK_SOURCE_ROW_COLUMN.to_string());
    output_headers.push(LINK_SOURCE_ORDINAL_COLUMN.to_string());
    writer.write_record(&output_headers).map_err(|error| {
        link_io_refusal(
            "Failed to write entity link materialized headers",
            output,
            error,
        )
    })?;

    let reference_count = append_side_rows(
        EntityLinkRole::Reference,
        &reference,
        &merged_headers,
        &mut writer,
        output,
    )?;
    let target_count = append_side_rows(
        EntityLinkRole::Target,
        &target,
        &merged_headers,
        &mut writer,
        output,
    )?;
    writer.flush().map_err(|error| {
        link_io_refusal(
            "Failed to flush entity link materialized rows",
            output,
            error,
        )
    })?;

    Ok(MaterializedLinkRows {
        reference_rows: reference_count,
        target_rows: target_count,
    })
}

fn empty_link_decision_artifact() -> EntityLinkDecisionArtifact {
    EntityLinkDecisionArtifact {
        version: ENTITY_LINK_DECISIONS_VERSION.to_string(),
        artifact_content_hash: String::new(),
        strategy: StrategyReference::default(),
        registry: ResolveRegistrySnapshot::default(),
        reference_tape: TapeSummary::default(),
        target_tape: TapeSummary::default(),
        summary: ResolveSummary::default(),
        matches: Vec::new(),
        unmatched: Vec::new(),
        ambiguous: Vec::new(),
        conflict_warnings: Vec::new(),
        gold_score: None,
        write_back: None,
    }
}

fn link_decision_artifact(
    decisions: &crate::resolve::ResolveArtifact,
) -> Result<EntityLinkDecisionArtifact, Refusal> {
    let mut artifact = EntityLinkDecisionArtifact {
        version: ENTITY_LINK_DECISIONS_VERSION.to_string(),
        artifact_content_hash: String::new(),
        strategy: decisions.strategy.clone(),
        registry: decisions.registry.clone(),
        reference_tape: decisions.reference_tape.clone(),
        target_tape: decisions.target_tape.clone(),
        summary: decisions.summary.clone(),
        matches: decisions.matches.clone(),
        unmatched: decisions.unmatched.clone(),
        ambiguous: decisions.ambiguous.clone(),
        conflict_warnings: decisions.conflict_warnings.clone(),
        gold_score: decisions.gold_score.clone(),
        write_back: decisions.write_back.clone(),
    };
    artifact.artifact_content_hash = hash_link_decision_artifact_without_self(&artifact)?;
    let bytes = serde_json::to_vec(&artifact).map_err(|error| {
        link_artifact_refusal(
            "Failed to stabilize entity link decision artifact hash",
            json!({
                "stage": "link",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let mut round_tripped: EntityLinkDecisionArtifact =
        serde_json::from_slice(&bytes).map_err(|error| {
            link_artifact_refusal(
                "Failed to stabilize entity link decision artifact hash",
                json!({
                    "stage": "link",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
    round_tripped.artifact_content_hash = hash_link_decision_artifact_without_self(&round_tripped)?;
    Ok(round_tripped)
}

fn link_decision_artifact_and_summary(
    decisions: &crate::resolve::ResolveArtifact,
) -> Result<(EntityLinkDecisionArtifact, ResolveSummary), Refusal> {
    let decision_artifact = link_decision_artifact(decisions)?;
    let summary = decision_artifact.summary.clone();
    Ok((decision_artifact, summary))
}

fn validate_link_summary(artifact: &EntityLinkArtifact) -> Result<(), Refusal> {
    if !artifact.summary.partition_holds()
        || artifact.summary.target_records != artifact.target.row_count as usize
        || artifact.summary.matched != artifact.decision_artifact.matches.len()
        || artifact.summary.unmatched != artifact.decision_artifact.unmatched.len()
        || artifact.summary.ambiguous != artifact.decision_artifact.ambiguous.len()
    {
        return Err(link_artifact_refusal(
            "Entity link partition counts do not match decision records",
            json!({
                "stage": "link",
                "field": "summary",
                "target_row_count": artifact.target.row_count,
                "summary": artifact.summary,
                "matches": artifact.decision_artifact.matches.len(),
                "unmatched": artifact.decision_artifact.unmatched.len(),
                "ambiguous": artifact.decision_artifact.ambiguous.len(),
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_link_decision_artifact(artifact: &EntityLinkDecisionArtifact) -> Result<(), Refusal> {
    if artifact.version != ENTITY_LINK_DECISIONS_VERSION {
        return Err(link_artifact_refusal(
            "Entity link decision artifact has the wrong contract version",
            json!({
                "stage": "link",
                "field": "decision_artifact.version",
                "expected": ENTITY_LINK_DECISIONS_VERSION,
                "actual": artifact.version,
                "writes_performed": false
            }),
        ));
    }
    if !artifact.summary.partition_holds()
        || artifact.summary.matched != artifact.matches.len()
        || artifact.summary.unmatched != artifact.unmatched.len()
        || artifact.summary.ambiguous != artifact.ambiguous.len()
    {
        return Err(link_artifact_refusal(
            "Entity link decision partition counts do not match records",
            json!({
                "stage": "link",
                "field": "decision_artifact.summary",
                "writes_performed": false
            }),
        ));
    }
    let expected = hash_link_decision_artifact_without_self(artifact)?;
    if artifact.artifact_content_hash != expected {
        return Err(link_artifact_refusal(
            "Entity link decision artifact content hash does not match its payload",
            json!({
                "stage": "link",
                "field": "decision_artifact.artifact_content_hash",
                "expected": expected,
                "actual": artifact.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn build_link_observation_surface_bindings(
    work_dir: &Path,
    artifact: &EntityLinkArtifact,
    profile_context_dirs: &[PathBuf],
    run_artifact: &EntityRunArtifact,
    decisions: &crate::resolve::ResolveArtifact,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    let materialized_path = materialized_rows_path(work_dir);
    build_link_observation_surface_bindings_from_materialized(
        work_dir,
        &materialized_path,
        artifact,
        profile_context_dirs,
        run_artifact,
        Some(decisions.strategy.content_hash.as_str()),
        "decision_artifact.strategy.content_hash",
    )
}

fn build_link_observation_surface_bindings_from_materialized(
    work_dir: &Path,
    materialized_path: &Path,
    artifact: &EntityLinkArtifact,
    profile_context_dirs: &[PathBuf],
    run_artifact: &EntityRunArtifact,
    expected_strategy_hash: Option<&str>,
    expected_strategy_hash_field: &'static str,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    let materialized_bytes = read_link_stable_bytes(materialized_path, "materialized rows")?;
    build_link_observation_surface_bindings_from_materialized_bytes(
        &materialized_bytes,
        LinkObservationSurfaceBindingBuildContext {
            work_dir,
            materialized_path,
            artifact,
            profile_context_dirs,
            run_artifact,
            expected_strategy_hash,
            expected_strategy_hash_field,
        },
    )
}

struct LinkObservationSurfaceBindingBuildContext<'a> {
    work_dir: &'a Path,
    materialized_path: &'a Path,
    artifact: &'a EntityLinkArtifact,
    profile_context_dirs: &'a [PathBuf],
    run_artifact: &'a EntityRunArtifact,
    expected_strategy_hash: Option<&'a str>,
    expected_strategy_hash_field: &'static str,
}

fn build_link_observation_surface_bindings_from_materialized_bytes(
    materialized_bytes: &[u8],
    context: LinkObservationSurfaceBindingBuildContext<'_>,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    let LinkObservationSurfaceBindingBuildContext {
        work_dir,
        materialized_path,
        artifact,
        profile_context_dirs,
        run_artifact,
        expected_strategy_hash,
        expected_strategy_hash_field,
    } = context;
    let identity = load_link_strategy_identity(
        run_artifact,
        profile_context_dirs,
        expected_strategy_hash,
        expected_strategy_hash_field,
    )?;
    let loaded_profile = validate_link_profile_source_against_run(
        &artifact.profile_source,
        profile_context_dirs,
        run_artifact,
    )?;
    let contract = prepare_contract_for_link_profile(&loaded_profile)?;
    let observations = project_prepare_csv_reader(Cursor::new(materialized_bytes), b',', &contract)
        .map_err(|refusal| {
            link_artifact_refusal(
                "Failed to replay prepared observations for entity link bindings",
                json!({
                    "stage": "link",
                    "path": materialized_path.display().to_string(),
                    "refusal": refusal,
                    "writes_performed": false
                }),
            )
        })?;
    let surfaces = read_link_run_surfaces(work_dir, run_artifact, &loaded_profile)?;
    let rows =
        read_materialized_binding_rows_bytes(materialized_bytes, materialized_path, &identity)?;
    if rows.len() != observations.len() {
        return Err(link_artifact_refusal(
            "Entity link binding replay row count does not match prepared observation count",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "materialized_rows": rows.len(),
                "prepared_observations": observations.len(),
                "writes_performed": false
            }),
        ));
    }

    let mut bindings = Vec::with_capacity(rows.len());
    for (row, observation) in rows.iter().zip(observations.iter()) {
        let surface = surface_for_observation(observation, &surfaces)?;
        bindings.push(EntityLinkObservationSurfaceBinding {
            version: ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION.to_string(),
            side: row.side,
            link_id: row.link_id.clone(),
            source_row_id: row.source_row_id.clone(),
            source_ordinal: row.source_ordinal,
            surface_id: surface.surface_id.clone(),
            profile_id: surface.profile_id.clone(),
            surface_binding_hash: surface_binding_hash(surface)?,
        });
    }
    bindings.sort_by(observation_surface_binding_cmp);
    Ok(bindings)
}

#[derive(Debug, Deserialize)]
struct LinkStrategyIdentityDocument {
    identity: LinkStrategyIdentity,
}

#[derive(Debug, Deserialize)]
struct LinkStrategyIdentity {
    reference: LinkStrategyIdentitySide,
    target: LinkStrategyIdentitySide,
}

#[derive(Debug, Deserialize)]
struct LinkStrategyIdentitySide {
    #[serde(default)]
    id_columns: Vec<String>,
}

fn load_link_strategy_identity(
    run_artifact: &EntityRunArtifact,
    context_dirs: &[PathBuf],
    expected_strategy_hash: Option<&str>,
    expected_strategy_hash_field: &'static str,
) -> Result<LinkStrategyIdentity, Refusal> {
    let strategy_source = run_artifact
        .summary
        .labels
        .get("strategy_source")
        .ok_or_else(|| {
            link_artifact_refusal(
                "Entity link run artifact does not record a strategy source for bindings",
                json!({
                    "stage": "link",
                    "field": "run.summary.labels.strategy_source",
                    "writes_performed": false
                }),
            )
        })?;
    let Some(expected_strategy_hash) =
        expected_strategy_hash.filter(|strategy_hash| !strategy_hash.trim().is_empty())
    else {
        return Err(link_artifact_refusal(
            "Entity link run artifact does not bind a strategy hash for bindings",
            json!({
                "stage": "link",
                "field": expected_strategy_hash_field,
                "writes_performed": false
            }),
        ));
    };
    let candidates = link_context_source_candidates(strategy_source, context_dirs);
    let mut mismatches = Vec::new();
    let mut load_failures = Vec::new();
    for candidate in &candidates {
        let bytes = match fs::read(candidate) {
            Ok(bytes) => bytes,
            Err(error) => {
                load_failures.push(json!({
                    "resolved_source": candidate,
                    "error": error.to_string()
                }));
                continue;
            }
        };
        let actual_hash = witness::hash_bytes(&bytes);
        if actual_hash.as_str() != expected_strategy_hash {
            mismatches.push(json!({
                "resolved_source": candidate,
                "actual": actual_hash
            }));
            continue;
        }
        let document: LinkStrategyIdentityDocument =
            serde_yaml::from_slice(&bytes).map_err(|error| {
                link_artifact_refusal(
                    "Failed to parse entity link strategy identity for bindings",
                    json!({
                        "stage": "link",
                        "path": candidate,
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                )
            })?;
        validate_strategy_identity_side(&document.identity.reference, EntityLinkRole::Reference)?;
        validate_strategy_identity_side(&document.identity.target, EntityLinkRole::Target)?;
        return Ok(document.identity);
    }
    if !mismatches.is_empty() {
        return Err(link_artifact_refusal(
            "Entity link strategy hash does not match binding derivation source",
            json!({
                "stage": "link",
                "field": expected_strategy_hash_field,
                "source": strategy_source,
                "expected": expected_strategy_hash,
                "attempted_sources": candidates,
                "mismatches": mismatches,
                "load_failures": load_failures,
                "writes_performed": false
            }),
        ));
    }
    Err(link_artifact_refusal(
        "Failed to read entity link strategy for observation/surface bindings",
        json!({
            "stage": "link",
            "field": "run.summary.labels.strategy_source",
            "source": strategy_source,
            "attempted_sources": candidates,
            "load_failures": load_failures,
            "writes_performed": false
        }),
    ))
}

fn validate_strategy_identity_side(
    side: &LinkStrategyIdentitySide,
    role: EntityLinkRole,
) -> Result<(), Refusal> {
    if side
        .id_columns
        .iter()
        .any(|column| column.trim().is_empty())
        || side.id_columns.is_empty()
    {
        return Err(link_artifact_refusal(
            "Entity link strategy identity columns are required for bindings",
            json!({
                "stage": "link",
                "side": role.as_str(),
                "field": "identity.id_columns",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MaterializedBindingRow {
    side: EntityLinkRole,
    link_id: String,
    source_row_id: Option<String>,
    source_ordinal: u64,
}

fn read_materialized_binding_rows_bytes(
    bytes: &[u8],
    path: &Path,
    identity: &LinkStrategyIdentity,
) -> Result<Vec<MaterializedBindingRow>, Refusal> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(Cursor::new(bytes));
    let headers = reader
        .headers()
        .map_err(|error| {
            link_io_refusal(
                "Failed to read entity link materialized row headers",
                path,
                error,
            )
        })?
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|error| {
            link_io_refusal("Failed to parse entity link materialized row", path, error)
        })?;
        let values = headers
            .iter()
            .enumerate()
            .map(|(field_index, header)| {
                (
                    header.clone(),
                    record.get(field_index).unwrap_or_default().to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let row_number = index + 1;
        let side = materialized_row_side(&values, row_number)?;
        let id_columns = match side {
            EntityLinkRole::Reference => &identity.reference.id_columns,
            EntityLinkRole::Target => &identity.target.id_columns,
        };
        let link_id = materialized_row_link_id(&values, id_columns, side, row_number)?;
        let source_ordinal = materialized_row_source_ordinal(&values, row_number)?;
        rows.push(MaterializedBindingRow {
            side,
            link_id,
            source_row_id: Some(required_materialized_value(
                &values,
                LINK_SOURCE_ROW_COLUMN,
                row_number,
            )?),
            source_ordinal,
        });
    }
    Ok(rows)
}

fn materialized_row_side(
    values: &BTreeMap<String, String>,
    row_number: usize,
) -> Result<EntityLinkRole, Refusal> {
    match required_materialized_value(values, LINK_SOURCE_NAME_COLUMN, row_number)?.as_str() {
        "reference" => Ok(EntityLinkRole::Reference),
        "target" => Ok(EntityLinkRole::Target),
        actual => Err(link_artifact_refusal(
            "Entity link materialized row has an invalid source side",
            json!({
                "stage": "link",
                "row_number": row_number,
                "field": LINK_SOURCE_NAME_COLUMN,
                "actual": actual,
                "writes_performed": false
            }),
        )),
    }
}

fn materialized_row_link_id(
    values: &BTreeMap<String, String>,
    id_columns: &[String],
    side: EntityLinkRole,
    row_number: usize,
) -> Result<String, Refusal> {
    let mut parts = Vec::with_capacity(id_columns.len());
    for column in id_columns {
        let value = required_materialized_value(values, column, row_number)?;
        if value.contains(LINK_COMPOSITE_ID_SEPARATOR) {
            return Err(link_artifact_refusal(
                "Entity link identity value contains the reserved composite separator",
                json!({
                    "stage": "link",
                    "row_number": row_number,
                    "side": side.as_str(),
                    "field": column,
                    "separator": LINK_COMPOSITE_ID_SEPARATOR,
                    "writes_performed": false
                }),
            ));
        }
        parts.push(value);
    }
    Ok(parts.join(LINK_COMPOSITE_ID_SEPARATOR))
}

fn materialized_row_source_ordinal(
    values: &BTreeMap<String, String>,
    row_number: usize,
) -> Result<u64, Refusal> {
    let value = required_materialized_value(values, LINK_SOURCE_ORDINAL_COLUMN, row_number)?;
    let ordinal = value.parse::<u64>().map_err(|error| {
        link_artifact_refusal(
            "Entity link materialized row source ordinal must be an integer",
            json!({
                "stage": "link",
                "row_number": row_number,
                "field": LINK_SOURCE_ORDINAL_COLUMN,
                "value": value,
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    if ordinal == 0 {
        return Err(link_artifact_refusal(
            "Entity link materialized row source ordinal must be positive",
            json!({
                "stage": "link",
                "row_number": row_number,
                "field": LINK_SOURCE_ORDINAL_COLUMN,
                "writes_performed": false
            }),
        ));
    }
    Ok(ordinal)
}

fn required_materialized_value(
    values: &BTreeMap<String, String>,
    field: &str,
    row_number: usize,
) -> Result<String, Refusal> {
    let value = values.get(field).ok_or_else(|| {
        link_artifact_refusal(
            "Entity link materialized row is missing a required binding field",
            json!({
                "stage": "link",
                "row_number": row_number,
                "field": field,
                "writes_performed": false
            }),
        )
    })?;
    let value = value.trim();
    if value.is_empty() {
        return Err(link_artifact_refusal(
            "Entity link materialized row has an empty required binding field",
            json!({
                "stage": "link",
                "row_number": row_number,
                "field": field,
                "writes_performed": false
            }),
        ));
    }
    Ok(value.to_string())
}

fn surface_for_observation<'a>(
    observation: &PreparedInputObservation,
    surfaces: &'a [PreparedSurfaceRecord],
) -> Result<&'a PreparedSurfaceRecord, Refusal> {
    let matches = surfaces
        .iter()
        .filter(|surface| {
            surface.profile_id == observation.profile_id
                && surface
                    .raw_variants
                    .iter()
                    .any(|variant| variant == &observation.primary_surface.value)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [surface] => Ok(*surface),
        [] => Err(link_artifact_refusal(
            "Entity link binding replay could not find the prepared surface for a row",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "row_number": observation.row_number,
                "writes_performed": false
            }),
        )),
        _ => Err(link_artifact_refusal(
            "Entity link binding replay found multiple prepared surfaces for a row",
            json!({
                "stage": "link",
                "field": "observation_surface_bindings",
                "row_number": observation.row_number,
                "writes_performed": false
            }),
        )),
    }
}

fn surface_binding_hash(surface: &PreparedSurfaceRecord) -> Result<String, Refusal> {
    let bytes = serde_json::to_vec(&json!({
        "version": ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION,
        "profile_id": surface.profile_id,
        "surface_id": surface.surface_id,
    }))
    .map_err(|error| {
        link_artifact_refusal(
            "Failed to hash entity link surface binding",
            json!({
                "stage": "link",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn observation_surface_binding_cmp(
    left: &EntityLinkObservationSurfaceBinding,
    right: &EntityLinkObservationSurfaceBinding,
) -> std::cmp::Ordering {
    left.side
        .as_str()
        .cmp(right.side.as_str())
        .then_with(|| left.link_id.cmp(&right.link_id))
        .then_with(|| left.source_ordinal.cmp(&right.source_ordinal))
        .then_with(|| left.surface_id.cmp(&right.surface_id))
}

fn decision_target_ids(artifact: &EntityLinkDecisionArtifact) -> Result<BTreeSet<&str>, Refusal> {
    let mut ids = BTreeSet::new();
    for target_id in artifact
        .matches
        .iter()
        .map(|record| record.target_id.as_str())
        .chain(
            artifact
                .unmatched
                .iter()
                .map(|record| record.target_id.as_str()),
        )
        .chain(
            artifact
                .ambiguous
                .iter()
                .map(|record| record.target_id.as_str()),
        )
    {
        if !ids.insert(target_id) {
            return Err(link_artifact_refusal(
                "Entity link decision artifact classifies a target id more than once",
                json!({
                    "stage": "link",
                    "field": "decision_artifact",
                    "target_id": target_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(ids)
}

fn decision_reference_ids(artifact: &EntityLinkDecisionArtifact) -> BTreeSet<&str> {
    artifact
        .matches
        .iter()
        .map(|record| record.reference_id.as_str())
        .chain(
            artifact
                .ambiguous
                .iter()
                .flat_map(|record| record.candidates.iter())
                .map(|candidate| candidate.reference_id.as_str()),
        )
        .chain(
            artifact
                .unmatched
                .iter()
                .filter_map(|record| record.best_candidate.as_ref())
                .map(|candidate| candidate.reference_id.as_str()),
        )
        .collect()
}

fn validate_link_upstreams(artifact: &EntityLinkArtifact) -> Result<(), Refusal> {
    if artifact.shared_run_artifact.content_hash.trim().is_empty()
        || artifact
            .shared_solve_artifact
            .content_hash
            .trim()
            .is_empty()
    {
        return Err(link_artifact_refusal(
            "Entity link artifact must bind shared run and solve artifacts",
            json!({
                "stage": "link",
                "field": "shared_artifacts",
                "writes_performed": false
            }),
        ));
    }
    let publication_parent = EntityArtifactReference {
        version: CANON_ENTITY_STAGE_PUBLICATION_VERSION.to_string(),
        content_hash: link_publication_parent_generation_id(artifact)?,
    };
    let mut expected = vec![
        artifact.shared_run_artifact.clone(),
        artifact.shared_solve_artifact.clone(),
        publication_parent,
    ];
    expected.sort_by(artifact_ref_cmp);
    let mut actual = artifact.metadata.upstream_artifacts.clone();
    actual.sort_by(artifact_ref_cmp);
    if actual != expected {
        return Err(link_artifact_refusal(
            "Entity link metadata upstream references must match shared artifacts",
            json!({
                "stage": "link",
                "field": "metadata.upstream_artifacts",
                "expected": expected,
                "actual": actual,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn load_link_assignment_alignment_artifacts(
    work_dir: &Path,
) -> Result<(Vec<EntityLinkAssignmentAlignmentArtifact>, Option<Vec<u8>>), Refusal> {
    let source_path = work_dir.join(ASSIGNMENT_ALIGNMENT_PATH);
    if !source_path.exists() {
        return Ok((Vec::new(), None));
    }
    let bytes = fs::read(&source_path).map_err(|error| {
        link_io_refusal(
            "Failed to read entity link assignment alignment artifact",
            &source_path,
            error,
        )
    })?;
    let sidecar = validate_assignment_alignment_artifact_bytes(&bytes)?;
    validate_assignment_alignment_evidence_binding(&sidecar, work_dir)?;
    Ok((
        vec![EntityLinkAssignmentAlignmentArtifact {
            version: ASSIGNMENT_ALIGNMENT_VERSION.to_string(),
            path: LINK_ASSIGNMENT_ALIGNMENT_PATH.to_string(),
            content_hash: witness::hash_bytes(&bytes),
            evidence_semantics: "nonidentity_relation_hint".to_string(),
        }],
        Some(bytes),
    ))
}

fn validate_link_assignment_alignment_artifacts(
    artifact: &EntityLinkArtifact,
) -> Result<(), Refusal> {
    let mut paths = BTreeSet::new();
    for reference in &artifact.assignment_alignment_artifacts {
        if reference.version != ASSIGNMENT_ALIGNMENT_VERSION {
            return Err(link_artifact_refusal(
                "Entity link assignment alignment artifact has the wrong contract version",
                json!({
                    "stage": "link",
                    "field": "assignment_alignment_artifacts.version",
                    "expected": ASSIGNMENT_ALIGNMENT_VERSION,
                    "actual": reference.version,
                    "writes_performed": false
                }),
            ));
        }
        validate_safe_relative_path(&reference.path, "assignment_alignment_artifacts.path")?;
        if reference.content_hash.trim().is_empty() {
            return Err(link_artifact_refusal(
                "Entity link assignment alignment artifact must carry a content hash",
                json!({
                    "stage": "link",
                    "field": "assignment_alignment_artifacts.content_hash",
                    "writes_performed": false
                }),
            ));
        }
        if reference.evidence_semantics != "nonidentity_relation_hint" {
            return Err(link_artifact_refusal(
                "Entity link assignment alignment artifact must remain nonidentity evidence",
                json!({
                    "stage": "link",
                    "field": "assignment_alignment_artifacts.evidence_semantics",
                    "expected": "nonidentity_relation_hint",
                    "actual": reference.evidence_semantics,
                    "writes_performed": false
                }),
            ));
        }
        if !paths.insert(reference.path.as_str()) {
            return Err(link_artifact_refusal(
                "Entity link assignment alignment artifacts must have unique paths",
                json!({
                    "stage": "link",
                    "field": "assignment_alignment_artifacts.path",
                    "path": reference.path,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn validate_link_assignment_alignment_artifacts_at_path(
    artifact: &EntityLinkArtifact,
    artifact_path: &Path,
) -> Result<(), Refusal> {
    let base_dir = entity_link_artifact_base_dir(artifact_path);
    let work_dir = entity_link_work_dir_from_artifact_path(artifact_path);
    for reference in &artifact.assignment_alignment_artifacts {
        let path = base_dir.join(&reference.path);
        let bytes =
            read_link_committed_or_stable_bytes(&work_dir, &path, "assignment alignment artifact")?;
        let sidecar = validate_assignment_alignment_artifact_bytes(&bytes)?;
        let actual_hash = witness::hash_bytes(&bytes);
        if actual_hash != reference.content_hash {
            return Err(link_artifact_refusal(
                "Entity link assignment alignment hash does not match the linked payload",
                json!({
                    "stage": "link",
                    "field": "assignment_alignment_artifacts.content_hash",
                    "path": path.display().to_string(),
                    "expected": reference.content_hash,
                    "actual": actual_hash,
                    "writes_performed": false
                }),
            ));
        }
        validate_assignment_alignment_evidence_binding(&sidecar, &work_dir)?;
    }
    Ok(())
}

fn validate_assignment_alignment_artifact_bytes(
    bytes: &[u8],
) -> Result<AssignmentAlignmentSidecar, Refusal> {
    let sidecar: AssignmentAlignmentSidecar = serde_json::from_slice(bytes).map_err(|error| {
        link_artifact_refusal(
            "Failed to parse entity link assignment alignment artifact",
            json!({
                "stage": "link",
                "field": "assignment_alignment_artifacts",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    validate_assignment_alignment_sidecar(&sidecar).map_err(|error| {
        link_artifact_refusal(
            "Entity link assignment alignment artifact violates the record-link contract",
            json!({
                "stage": "link",
                "field": "assignment_alignment_artifacts",
                "record_link_stage": error.stage,
                "reason": error.reason,
                "error": error.message,
                "writes_performed": false
            }),
        )
    })?;
    let canonical = canonical_assignment_alignment_bytes(&sidecar).map_err(|error| {
        link_artifact_refusal(
            "Entity link assignment alignment artifact cannot be canonically serialized",
            json!({
                "stage": "link",
                "field": "assignment_alignment_artifacts",
                "record_link_stage": error.stage,
                "reason": error.reason,
                "error": error.message,
                "writes_performed": false
            }),
        )
    })?;
    if canonical != bytes {
        return Err(link_artifact_refusal(
            "Entity link assignment alignment artifact bytes are not canonical",
            json!({
                "stage": "link",
                "field": "assignment_alignment_artifacts",
                "writes_performed": false
            }),
        ));
    }
    Ok(sidecar)
}

fn validate_assignment_alignment_evidence_binding(
    sidecar: &AssignmentAlignmentSidecar,
    work_dir: &Path,
) -> Result<(), Refusal> {
    if sidecar.record_link_evidence_path != RECORD_LINK_EVIDENCE_PATH {
        return Err(link_artifact_refusal(
            "Entity link assignment alignment artifact must bind canonical record-link evidence",
            json!({
                "stage": "link",
                "field": "assignment_alignment_artifacts.record_link_evidence_path",
                "expected": RECORD_LINK_EVIDENCE_PATH,
                "actual": &sidecar.record_link_evidence_path,
                "writes_performed": false
            }),
        ));
    }
    let evidence_path = work_dir.join(&sidecar.record_link_evidence_path);
    let bytes = read_link_committed_or_stable_bytes(
        work_dir,
        &evidence_path,
        "record-link evidence artifact",
    )?;
    let bundle: EvidenceBundle = serde_json::from_slice(&bytes).map_err(|error| {
        link_artifact_refusal(
            "Failed to parse entity link record-link evidence artifact",
            json!({
                "stage": "link",
                "field": "assignment_alignment_artifacts.record_link_evidence_hash",
                "path": evidence_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    canonical_evidence_bundle_bytes(&bundle).map_err(|error| {
        link_artifact_refusal(
            "Entity link record-link evidence artifact violates the evidence contract",
            json!({
                "stage": "link",
                "field": "assignment_alignment_artifacts.record_link_evidence_hash",
                "path": evidence_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    if bundle.content_hash.as_str() != sidecar.record_link_evidence_hash.as_str() {
        return Err(link_artifact_refusal(
            "Entity link assignment alignment evidence hash does not match record-link evidence",
            json!({
                "stage": "link",
                "field": "assignment_alignment_artifacts.record_link_evidence_hash",
                "path": evidence_path.display().to_string(),
                "expected": &sidecar.record_link_evidence_hash,
                "actual": &bundle.content_hash,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn entity_link_work_dir_from_artifact_path(artifact_path: &Path) -> PathBuf {
    let base_dir = entity_link_artifact_base_dir(artifact_path);
    if base_dir.file_name().and_then(|name| name.to_str()) == Some("link") {
        base_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| base_dir.to_path_buf())
    } else {
        base_dir.to_path_buf()
    }
}

fn solve_stage_reference(run: &EntityRunArtifact) -> Result<EntityArtifactReference, Refusal> {
    run.stage_artifacts
        .iter()
        .find(|stage| stage.stage == "solve" && stage.version == CANON_ENTITY_SOLVE_VERSION_V1)
        .map(|stage| EntityArtifactReference {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .ok_or_else(|| {
            link_artifact_refusal(
                "Entity link requires a shared solve artifact reference",
                json!({
                    "stage": "link",
                    "field": "shared_solve_artifact",
                    "writes_performed": false
                }),
            )
        })
}

fn hash_link_artifact_without_self(artifact: &EntityLinkArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        link_artifact_refusal(
            "Failed to hash entity link artifact",
            json!({
                "stage": "link",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn hash_link_decision_artifact_without_self(
    artifact: &EntityLinkDecisionArtifact,
) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        link_artifact_refusal(
            "Failed to hash entity link decision artifact",
            json!({
                "stage": "link",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn read_link_stable_bytes(path: &Path, label: &'static str) -> Result<Vec<u8>, Refusal> {
    fs::read(path).map_err(|error| {
        link_io_refusal(format!("Failed to read entity link {label}"), path, error)
    })
}

fn read_link_committed_or_stable_bytes(
    work_dir: &Path,
    stable_path: &Path,
    label: &'static str,
) -> Result<Vec<u8>, Refusal> {
    match read_entity_run_committed_publication_stable_path_bytes(work_dir, stable_path)? {
        Some(bytes) => Ok(bytes),
        None => read_link_stable_bytes(stable_path, label),
    }
}

fn read_observation_surface_bindings_bytes(
    bytes: &[u8],
    path: &Path,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    let reader = BufReader::new(Cursor::new(bytes));
    let mut bindings = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| {
            link_io_refusal(
                "Failed to read entity link observation/surface binding",
                path,
                error,
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let binding = serde_json::from_str::<EntityLinkObservationSurfaceBinding>(&line).map_err(
            |error| {
                link_artifact_refusal(
                    "Failed to parse entity link observation/surface binding",
                    json!({
                        "stage": "link",
                        "path": path.display().to_string(),
                        "line": index + 1,
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                )
            },
        )?;
        bindings.push(binding);
    }
    Ok(bindings)
}

fn validate_safe_relative_path(value: &str, field: &'static str) -> Result<(), Refusal> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(link_artifact_refusal(
            "Entity link artifact path must be a safe relative path",
            json!({
                "stage": "link",
                "field": field,
                "path": value,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn json_bytes<T: Serialize>(value: &T, label: &'static str) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec(value).map_err(|error| {
        link_artifact_refusal(
            "Failed to serialize entity link artifact",
            json!({
                "stage": "link",
                "artifact": label,
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

fn jsonl_bytes<T: Serialize>(values: &[T], label: &'static str) -> Result<Vec<u8>, Refusal> {
    let mut bytes = Vec::new();
    for value in values {
        serde_json::to_writer(&mut bytes, value).map_err(|error| {
            link_artifact_refusal(
                "Failed to serialize entity link observation/surface binding",
                json!({
                    "stage": "link",
                    "artifact": label,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn artifact_ref_cmp(
    left: &EntityArtifactReference,
    right: &EntityArtifactReference,
) -> std::cmp::Ordering {
    left.version
        .cmp(&right.version)
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn link_artifact_refusal(message: impl Into<String>, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("Use canon entity link to regenerate link/link.json".to_string()),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkInputRows {
    headers: Vec<String>,
    records: Vec<LinkInputRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkInputRecord {
    values: BTreeMap<String, String>,
    source_row_id: Option<String>,
}

fn load_link_input_rows(path: &Path, role: EntityLinkRole) -> Result<LinkInputRows, Refusal> {
    if path == Path::new("-") {
        return Err(link_input_refusal(
            "Entity link rows must be filesystem paths, not stdin",
            path,
            role,
            "stdin is not accepted for entity link materialization".to_string(),
        ));
    }

    let format = input::detect_format(path).map_err(|error| {
        link_input_refusal(
            "Unsupported entity link row input format",
            path,
            role,
            error.to_string(),
        )
    })?;

    let rows = match format {
        InputFormat::Csv => load_delimited_link_rows(path, role)?,
        InputFormat::Jsonl => load_jsonl_link_rows(path, role)?,
    };
    ensure_no_reserved_link_columns(&rows.headers, path, role)?;
    Ok(rows)
}

fn load_delimited_link_rows(path: &Path, role: EntityLinkRole) -> Result<LinkInputRows, Refusal> {
    let file = File::open(path).map_err(|error| {
        link_input_refusal(
            "Failed to open entity link delimited rows",
            path,
            role,
            error.to_string(),
        )
    })?;
    let delimiter = input::detect_csv_delimiter(&file).map_err(|error| {
        link_input_refusal(
            "Failed to detect entity link row delimiter",
            path,
            role,
            error.to_string(),
        )
    })?;
    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(file);
    let headers = reader
        .headers()
        .map_err(|error| {
            link_input_refusal(
                "Failed to read entity link delimited headers",
                path,
                role,
                error.to_string(),
            )
        })?
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

    let mut records = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| {
            link_input_refusal(
                "Failed to parse entity link delimited record",
                path,
                role,
                error.to_string(),
            )
        })?;
        let values = headers
            .iter()
            .enumerate()
            .map(|(index, header)| {
                (
                    header.clone(),
                    record.get(index).unwrap_or_default().to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        records.push(LinkInputRecord {
            source_row_id: source_row_id_from_values(&values),
            values,
        });
    }

    Ok(LinkInputRows { headers, records })
}

fn load_jsonl_link_rows(path: &Path, role: EntityLinkRole) -> Result<LinkInputRows, Refusal> {
    let file = File::open(path).map_err(|error| {
        link_input_refusal(
            "Failed to open entity link JSONL rows",
            path,
            role,
            error.to_string(),
        )
    })?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut headers = BTreeSet::new();
    let mut records = Vec::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).map_err(|error| {
            link_input_refusal(
                "Failed to read entity link JSONL row",
                path,
                role,
                error.to_string(),
            )
        })?;
        if bytes_read == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }

        let row_index = records.len() + 1;
        let value: Value = serde_json::from_str(&line).map_err(|error| {
            link_input_refusal(
                "Failed to parse entity link JSONL row",
                path,
                role,
                format!("line {row_index}: {error}"),
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            link_input_refusal(
                "Entity link JSONL row must be an object",
                path,
                role,
                format!("line {row_index} is not a JSON object"),
            )
        })?;
        let mut values = BTreeMap::new();
        for (key, value) in object {
            headers.insert(key.clone());
            values.insert(key.clone(), json_link_cell(value));
        }
        records.push(LinkInputRecord {
            source_row_id: source_row_id_from_values(&values),
            values,
        });
    }

    Ok(LinkInputRows {
        headers: headers.into_iter().collect(),
        records,
    })
}

fn json_link_cell(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
        }
    }
}

fn append_side_rows<W: std::io::Write>(
    role: EntityLinkRole,
    rows: &LinkInputRows,
    merged_headers: &[String],
    writer: &mut csv::Writer<W>,
    output_path: &Path,
) -> Result<u64, Refusal> {
    let mut count = 0_u64;
    for record in &rows.records {
        count += 1;
        let mut output = merged_headers
            .iter()
            .map(|header| record.values.get(header).cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        output.push(role.as_str().to_string());
        output.push(role.as_str().to_string());
        output.push(
            record
                .source_row_id
                .clone()
                .unwrap_or_else(|| count.to_string()),
        );
        output.push(count.to_string());
        writer.write_record(&output).map_err(|error| {
            link_io_refusal(
                "Failed to write entity link materialized row",
                output_path,
                error,
            )
        })?;
    }
    Ok(count)
}

fn merged_headers(reference: &[String], target: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    reference
        .iter()
        .chain(target.iter())
        .filter_map(|header| {
            let header = header.to_string();
            seen.insert(header.clone()).then_some(header)
        })
        .collect()
}

fn ensure_no_reserved_link_columns(
    headers: &[String],
    path: &Path,
    role: EntityLinkRole,
) -> Result<(), Refusal> {
    for reserved in [
        LINK_SIDE_COLUMN,
        LINK_SOURCE_NAME_COLUMN,
        LINK_SOURCE_ROW_COLUMN,
        LINK_SOURCE_ORDINAL_COLUMN,
    ] {
        if headers.iter().any(|header| header == reserved) {
            return Err(EntityRefusalKind::InputContract.to_refusal(
                "Entity link input already contains reserved link metadata columns",
                json!({
                    "stage": "link",
                    "role": role.as_str(),
                    "path": path.display().to_string(),
                    "column": reserved,
                    "writes_performed": false
                }),
                Some("Remove reserved canon_link_* columns before running entity link".to_string()),
            ));
        }
    }
    Ok(())
}

fn source_row_id_from_values(values: &BTreeMap<String, String>) -> Option<String> {
    values
        .get("source_row_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn link_input_refusal(
    message: &'static str,
    path: &Path,
    role: EntityLinkRole,
    error: String,
) -> Refusal {
    EntityRefusalKind::InputContract.to_refusal(
        message,
        json!({
            "stage": "link",
            "role": role.as_str(),
            "path": path.display().to_string(),
            "error": error,
            "writes_performed": false
        }),
        Some("Fix entity link input rows, then rerun canon entity link".to_string()),
    )
}

fn link_io_refusal(
    message: impl Into<String>,
    path: &Path,
    error: impl std::fmt::Display,
) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        message,
        json!({
            "stage": "link",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        Some("Check entity link work-dir permissions, then rerun canon entity link".to_string()),
    )
}

#[cfg(test)]
mod cache_runtime_tests {
    use super::*;
    use crate::entity::index::EntityIndexCacheStatus;
    use crate::entity::index_io::{
        CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION, EntityIndexCacheReceipt, INDEX_CACHE_RECEIPT_FILE,
    };
    use crate::entity::run::{
        EntityRunNextCommands, EntityRunOrchestration, EntityRunWorkDirLayout,
        RUN_CACHE_EXECUTION_RECEIPT_PATH,
    };
    use crate::entity::{
        EntityArtifactMetadata, EntityDeterministicSummary, EntityStrategyReference,
    };
    use crate::resolve::ResolveArtifact;
    use std::path::PathBuf;

    const STRATEGY_IDENTITY_YAML: &str = "\
strategy_id: context-strategy
strategy_version: 1.0.0
identity:
  reference:
    id_columns:
      - reference_id
  target:
    id_columns:
      - target_id
";

    struct LinkCacheFixture {
        reference_rows: PathBuf,
        target_rows: PathBuf,
        profile: PathBuf,
        strategy: PathBuf,
        registry: PathBuf,
    }

    impl LinkCacheFixture {
        fn load() -> Self {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
                "tests/fixtures/extensions/neutral-domain/time_forward/trials/entity_disjoint/source",
            );
            Self {
                reference_rows: root.join("reference_rows.csv"),
                target_rows: root.join("target_rows.csv"),
                profile: root.join("profile/regab_firm_identity.yaml"),
                strategy: root.join("link_strategy.yaml"),
                registry: root.join("registry"),
            }
        }

        fn request<'a>(&'a self, work_dir: &'a Path) -> EntityLinkRequest<'a> {
            EntityLinkRequest {
                reference_rows: &self.reference_rows,
                target_rows: &self.target_rows,
                profile: self
                    .profile
                    .to_str()
                    .expect("fixture profile path is UTF-8"),
                strategy: &self.strategy,
                registry: &self.registry,
                work_dir,
            }
        }
    }

    fn minimal_run_artifact_with_strategy_source(
        strategy_source: &str,
        strategy_hash: &str,
    ) -> EntityRunArtifact {
        let metadata = EntityArtifactMetadata {
            strategy: EntityStrategyReference {
                id: "context-strategy".to_string(),
                version: "1.0.0".to_string(),
                content_hash: strategy_hash.to_string(),
            },
            ..Default::default()
        };
        let mut summary = EntityDeterministicSummary::default();
        summary
            .labels
            .insert("strategy_source".to_string(), strategy_source.to_string());
        EntityRunArtifact {
            version: "canon_entity_run.v1".to_string(),
            artifact_content_hash: "blake3:test-run".to_string(),
            metadata,
            summary,
            stage_artifacts: Vec::new(),
            work_dir: EntityRunWorkDirLayout {
                prepare_artifact_path: "prepare/prepare.json".to_string(),
                surfaces_path: "prepare/surfaces.jsonl".to_string(),
                index_artifact_path: "index/index.json".to_string(),
                block_artifact_path: "block/block.json".to_string(),
                candidate_records_path: "block/candidates.jsonl".to_string(),
                candidate_diagnostics_path: "block/diagnostics.json".to_string(),
                exact_bucket_assertions_path: "block/exact_buckets.jsonl".to_string(),
                edge_artifact_path: "evidence/evidence.json".to_string(),
                edge_records_path: "evidence/evidence.jsonl".to_string(),
                solve_artifact_path: "solve/solve.json".to_string(),
                decision_ledger_path: "solve/decision_ledger.jsonl".to_string(),
                run_artifact_path: "run/run.json".to_string(),
            },
            next_commands: EntityRunNextCommands {
                resume: String::new(),
                review_export: String::new(),
                audit: String::new(),
                promote: String::new(),
                apply: String::new(),
            },
            orchestration: EntityRunOrchestration::default(),
        }
    }

    #[test]
    fn top_level_summary_uses_stabilized_decision_summary() {
        let decisions = ResolveArtifact {
            summary: ResolveSummary {
                target_records: 11,
                matched: 1,
                unmatched: 10,
                ambiguous: 0,
                match_rate: 1.0 / 11.0,
            },
            ..ResolveArtifact::default()
        };

        let (decision_artifact, top_level_summary) =
            link_decision_artifact_and_summary(&decisions).expect("decision artifact stabilizes");

        assert!(top_level_summary.partition_holds());
        assert_eq!(top_level_summary, decision_artifact.summary);
        assert_eq!(
            top_level_summary.match_rate.to_bits(),
            decision_artifact.summary.match_rate.to_bits()
        );
    }

    #[test]
    fn strategy_identity_resolves_relative_source_from_artifact_context() {
        let temp = tempfile::tempdir().expect("tempdir");
        let trial_dir = temp.path().join("trial");
        let source_label = "context-only-source/link_strategy.yaml";
        let strategy_path = trial_dir.join(source_label);
        fs::create_dir_all(strategy_path.parent().expect("strategy parent"))
            .expect("create strategy parent");
        fs::write(&strategy_path, STRATEGY_IDENTITY_YAML).expect("write strategy");
        assert!(
            !Path::new(source_label).exists(),
            "raw persisted strategy label must not be readable from cwd"
        );

        let strategy_hash = witness::hash_bytes(STRATEGY_IDENTITY_YAML.as_bytes());
        let run_artifact = minimal_run_artifact_with_strategy_source(source_label, &strategy_hash);
        let artifact_path = trial_dir.join("work/link/link.json");
        let context_dirs = link_profile_source_context_dirs(&artifact_path);

        let identity = load_link_strategy_identity(
            &run_artifact,
            &context_dirs,
            Some(&strategy_hash),
            "run_artifact.metadata.strategy.content_hash",
        )
        .expect("strategy identity resolves through artifact context");

        assert_eq!(
            identity.reference.id_columns,
            vec!["reference_id".to_string()]
        );
        assert_eq!(identity.target.id_columns, vec!["target_id".to_string()]);
    }

    #[test]
    fn strategy_identity_context_source_still_refuses_wrong_hash() {
        let temp = tempfile::tempdir().expect("tempdir");
        let trial_dir = temp.path().join("trial");
        let source_label = "context-only-source/link_strategy.yaml";
        let strategy_path = trial_dir.join(source_label);
        fs::create_dir_all(strategy_path.parent().expect("strategy parent"))
            .expect("create strategy parent");
        fs::write(&strategy_path, STRATEGY_IDENTITY_YAML).expect("write strategy");

        let actual_hash = witness::hash_bytes(STRATEGY_IDENTITY_YAML.as_bytes());
        let wrong_hash = witness::hash_bytes(b"wrong strategy bytes");
        let run_artifact = minimal_run_artifact_with_strategy_source(source_label, &wrong_hash);
        let artifact_path = trial_dir.join("work/link/link.json");
        let context_dirs = link_profile_source_context_dirs(&artifact_path);

        let refusal = load_link_strategy_identity(
            &run_artifact,
            &context_dirs,
            Some(&wrong_hash),
            "run_artifact.metadata.strategy.content_hash",
        )
        .expect_err("wrong strategy hash refuses");

        assert!(
            refusal
                .message
                .contains("strategy hash does not match binding derivation source")
        );
        assert_eq!(
            refusal.detail["expected"].as_str(),
            Some(wrong_hash.as_str())
        );
        assert!(
            refusal
                .detail
                .get("mismatches")
                .and_then(Value::as_array)
                .expect("mismatches")
                .iter()
                .any(|mismatch| mismatch["actual"].as_str() == Some(actual_hash.as_str()))
        );
    }

    #[test]
    fn link_cache_mode_wrapper_passes_disabled_mode_to_nested_run() {
        let fixture = LinkCacheFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");

        let result = run_entity_link_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Disabled,
        )
        .expect("link runs with disabled cache mode");
        assert_eq!(result.run.artifact.summary.labels["cache_mode"], "disabled");
        assert_eq!(
            result.run.artifact.summary.labels["cache_status"],
            "bypassed"
        );
        let cache_stage = result
            .run
            .artifact
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "cache_disabled")
            .expect("cache_disabled stage");
        assert_eq!(
            cache_stage.version,
            CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION
        );
        assert_eq!(cache_stage.path, RUN_CACHE_EXECUTION_RECEIPT_PATH);
        assert_eq!(
            result.run.artifact.summary.labels["cache_receipt_path"],
            RUN_CACHE_EXECUTION_RECEIPT_PATH
        );
        assert_eq!(
            cache_stage.artifact_content_hash,
            result.run.artifact.summary.labels["cache_receipt_hash"]
        );
        let execution_receipt: EntityIndexCacheReceipt = serde_json::from_slice(
            &fs::read(work_dir.join(RUN_CACHE_EXECUTION_RECEIPT_PATH))
                .expect("cache execution receipt bytes"),
        )
        .expect("cache execution receipt parses");
        assert_eq!(execution_receipt.mode, EntityIndexCacheMode::Disabled);
        assert_eq!(execution_receipt.status, EntityIndexCacheStatus::Bypassed);
        assert!(!execution_receipt.reusable);
        assert_eq!(
            witness::hash_file(&work_dir.join(RUN_CACHE_EXECUTION_RECEIPT_PATH))
                .expect("cache execution receipt hashes"),
            cache_stage.artifact_content_hash
        );

        let bundle_receipt_path = &result.run.artifact.summary.labels["cache_bundle_receipt_path"];
        let bundle_receipt_hash = &result.run.artifact.summary.labels["cache_bundle_receipt_hash"];
        assert_eq!(bundle_receipt_path, INDEX_CACHE_RECEIPT_FILE);
        assert_eq!(
            witness::hash_file(&work_dir.join(INDEX_CACHE_RECEIPT_FILE))
                .expect("cache bundle receipt hashes"),
            *bundle_receipt_hash
        );
        let index_stage = result
            .run
            .artifact
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "index")
            .expect("index stage");
        assert_eq!(
            cache_stage.upstream_artifacts,
            vec![
                EntityArtifactReference {
                    version: index_stage.version.clone(),
                    content_hash: index_stage.artifact_content_hash.clone(),
                },
                EntityArtifactReference {
                    version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
                    content_hash: bundle_receipt_hash.clone(),
                },
            ]
        );
    }
}
