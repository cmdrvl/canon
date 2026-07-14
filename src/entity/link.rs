#![forbid(unsafe_code)]

//! Directional entity-link adapter over the shared native entity stages.
//!
//! Link mode keeps reference/target semantics in the materialized input rows,
//! then delegates indexing, blocking, evidence, solve, review, audit, promote,
//! and apply handoffs to `run_entity_workbench`.

use super::{EntityRunRequest, EntityRunResult, run_entity_workbench_with_cache_mode};
use crate::{
    InputFormat, Refusal,
    entity::index::EntityIndexCacheMode,
    entity::run::EntityRunArtifact,
    entity::{
        CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactReference,
        error::EntityRefusalKind,
        prepare::{
            PrepareInputContract, PreparedInputObservation, PreparedSurfaceRecord,
            load_prepare_profile, prepare_surface_records, project_prepare_path,
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
    io::{BufRead, BufReader, Write},
    path::{Component, Path, PathBuf},
};

pub mod multisource;

pub const ENTITY_LINK_VERSION: &str = "canon_entity_link.v0";
pub const ENTITY_LINK_DECISIONS_VERSION: &str = "canon_entity_link_decisions.v0";
pub const ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION: &str =
    "canon_entity_link_observation_surface_bindings.v0";
pub const LINK_SIDE_COLUMN: &str = "canon_link_side";
pub const LINK_SOURCE_NAME_COLUMN: &str = "canon_link_source_name";
pub const LINK_SOURCE_ROW_COLUMN: &str = "canon_link_source_row_id";
pub const LINK_SOURCE_ORDINAL_COLUMN: &str = "canon_link_source_ordinal";
pub const LINK_ARTIFACT_PATH: &str = "link/link.json";
pub const LINK_MATERIALIZED_ROWS_PATH: &str = "combined_rows.csv";
pub const LINK_OBSERVATION_SURFACE_BINDINGS_PATH: &str = "observation_surface_bindings.jsonl";
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
    pub observation_surface_bindings_path: String,
    pub observation_surface_bindings_content_hash: String,
    pub shared_run_artifact: EntityArtifactReference,
    pub shared_solve_artifact: EntityArtifactReference,
    pub decision_artifact: EntityLinkDecisionArtifact,
    pub next_commands: EntityLinkNextCommands,
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
    let mut metadata = run.artifact.metadata.clone();
    metadata.upstream_artifacts = vec![shared_run_artifact.clone(), shared_solve_artifact.clone()];
    metadata.upstream_artifacts.sort_by(artifact_ref_cmp);
    metadata.artifact_content_hash.clear();
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
        observation_surface_bindings_path: LINK_OBSERVATION_SURFACE_BINDINGS_PATH.to_string(),
        observation_surface_bindings_content_hash: String::new(),
        shared_run_artifact,
        shared_solve_artifact,
        decision_artifact: empty_link_decision_artifact(),
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

pub fn finalize_entity_link_artifact(
    request: EntityLinkFinalizeRequest<'_>,
) -> Result<EntityLinkArtifact, Refusal> {
    let mut artifact = request.artifact;
    let shared_run_artifact = EntityArtifactReference {
        version: request.run_artifact.version.clone(),
        content_hash: request.run_artifact.artifact_content_hash.clone(),
    };
    let shared_solve_artifact = solve_stage_reference(request.run_artifact)?;
    artifact.shared_run_artifact = shared_run_artifact.clone();
    artifact.shared_solve_artifact = shared_solve_artifact.clone();
    artifact.summary = request.decisions.summary.clone();
    artifact.materialized_rows_path = LINK_MATERIALIZED_ROWS_PATH.to_string();
    artifact.materialized_rows_content_hash = hash_file(
        &materialized_rows_path(request.work_dir),
        "materialized rows",
    )?;
    artifact.observation_surface_bindings_path = LINK_OBSERVATION_SURFACE_BINDINGS_PATH.to_string();
    let mut metadata = request.run_artifact.metadata.clone();
    metadata.upstream_artifacts = vec![shared_run_artifact, shared_solve_artifact];
    metadata.upstream_artifacts.sort_by(artifact_ref_cmp);
    metadata.artifact_content_hash.clear();
    artifact.metadata = metadata;
    artifact.decision_artifact = link_decision_artifact(request.decisions)?;
    let bindings = build_link_observation_surface_bindings(
        request.work_dir,
        request.run_artifact,
        request.decisions,
    )?;
    validate_entity_link_observation_surface_bindings(&artifact, &bindings)?;
    let bindings_path = observation_surface_bindings_path(request.work_dir);
    write_jsonl_file(&bindings_path, &bindings)?;
    artifact.observation_surface_bindings_content_hash =
        hash_file(&bindings_path, "observation/surface bindings")?;
    artifact.next_commands = EntityLinkNextCommands {
        review_export: format!(
            "canon entity review export {} --include escrow --emit csv",
            link_artifact_path(request.work_dir).display()
        ),
    };
    artifact.artifact_content_hash = hash_link_artifact_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    validate_entity_link_artifact_contract(&artifact)?;
    write_json_file(&link_artifact_path(request.work_dir), &artifact)?;
    Ok(artifact)
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
            "observation_surface_bindings_path",
            "observation_surface_bindings_content_hash",
            "shared_run_artifact",
            "shared_solve_artifact",
            "decision_artifact",
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
    Ok(())
}

pub fn validate_entity_link_artifact_at_path(
    artifact: &EntityLinkArtifact,
    artifact_path: &Path,
) -> Result<(), Refusal> {
    validate_entity_link_artifact_contract(artifact)?;
    let base_dir = entity_link_artifact_base_dir(artifact_path);
    let materialized_path = base_dir.join(&artifact.materialized_rows_path);
    let actual_hash = hash_file(&materialized_path, "materialized rows")?;
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
    read_validated_entity_link_observation_surface_bindings_at_path(artifact, artifact_path)?;
    Ok(())
}

pub fn read_validated_entity_link_observation_surface_bindings_at_path(
    artifact: &EntityLinkArtifact,
    artifact_path: &Path,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    validate_entity_link_artifact_contract(artifact)?;
    let base_dir = entity_link_artifact_base_dir(artifact_path);
    let bindings_path = base_dir.join(&artifact.observation_surface_bindings_path);
    let actual_hash = hash_file(&bindings_path, "observation/surface bindings")?;
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
    let bindings = read_observation_surface_bindings(&bindings_path)?;
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
    let materialized_hash = hash_file(&materialized_path, "materialized rows")?;
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
    let expected = build_link_observation_surface_bindings_from_materialized(
        &materialized_path,
        run_artifact,
        entity_link_run_strategy_hash(run_artifact),
        "run_artifact.metadata.strategy.content_hash",
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
    let expected_run_hash = hash_entity_run_artifact_without_self(run_artifact)?;
    if run_artifact.artifact_content_hash != expected_run_hash {
        return Err(link_artifact_refusal(
            "Entity run artifact content hash does not match its payload",
            json!({
                "stage": "link",
                "field": "run_artifact.artifact_content_hash",
                "expected": expected_run_hash,
                "actual": run_artifact.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
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

fn hash_entity_run_artifact_without_self(artifact: &EntityRunArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        link_artifact_refusal(
            "Failed to hash entity run artifact for link derivation validation",
            json!({
                "stage": "link",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
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
    run_artifact: &EntityRunArtifact,
    decisions: &crate::resolve::ResolveArtifact,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    let materialized_path = materialized_rows_path(work_dir);
    build_link_observation_surface_bindings_from_materialized(
        &materialized_path,
        run_artifact,
        Some(decisions.strategy.content_hash.as_str()),
        "decision_artifact.strategy.content_hash",
    )
}

fn build_link_observation_surface_bindings_from_materialized(
    materialized_path: &Path,
    run_artifact: &EntityRunArtifact,
    expected_strategy_hash: Option<&str>,
    expected_strategy_hash_field: &'static str,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    let identity = load_link_strategy_identity(
        run_artifact,
        expected_strategy_hash,
        expected_strategy_hash_field,
    )?;
    let profile_id = run_artifact
        .orchestration
        .profile_firewall
        .profile_id
        .as_str();
    let profile = load_prepare_profile(profile_id).map_err(|refusal| {
        link_artifact_refusal(
            "Failed to load entity link prepare profile for observation/surface bindings",
            json!({
                "stage": "link",
                "profile_id": profile_id,
                "refusal": refusal,
                "writes_performed": false
            }),
        )
    })?;
    let contract = PrepareInputContract::for_builtin_profile(&profile).map_err(|refusal| {
        link_artifact_refusal(
            "Failed to build entity link prepare contract for observation/surface bindings",
            json!({
                "stage": "link",
                "profile_id": profile_id,
                "refusal": refusal,
                "writes_performed": false
            }),
        )
    })?;
    let observations = project_prepare_path(materialized_path, &contract).map_err(|refusal| {
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
    let surfaces = prepare_surface_records(&observations).map_err(|refusal| {
        link_artifact_refusal(
            "Failed to replay prepared surfaces for entity link bindings",
            json!({
                "stage": "link",
                "refusal": refusal,
                "writes_performed": false
            }),
        )
    })?;
    let rows = read_materialized_binding_rows(materialized_path, &identity)?;
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
    let bytes = fs::read(strategy_source).map_err(|error| {
        link_io_refusal(
            "Failed to read entity link strategy for observation/surface bindings",
            Path::new(strategy_source),
            error,
        )
    })?;
    let actual_hash = witness::hash_bytes(&bytes);
    if let Some(expected_strategy_hash) =
        expected_strategy_hash.filter(|strategy_hash| !strategy_hash.trim().is_empty())
        && expected_strategy_hash != actual_hash
    {
        return Err(link_artifact_refusal(
            "Entity link strategy hash does not match binding derivation source",
            json!({
                "stage": "link",
                "field": expected_strategy_hash_field,
                "expected": expected_strategy_hash,
                "actual": actual_hash,
                "writes_performed": false
            }),
        ));
    }
    let document: LinkStrategyIdentityDocument =
        serde_yaml::from_slice(&bytes).map_err(|error| {
            link_artifact_refusal(
                "Failed to parse entity link strategy identity for bindings",
                json!({
                    "stage": "link",
                    "path": strategy_source,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
    validate_strategy_identity_side(&document.identity.reference, EntityLinkRole::Reference)?;
    validate_strategy_identity_side(&document.identity.target, EntityLinkRole::Target)?;
    Ok(document.identity)
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

fn read_materialized_binding_rows(
    path: &Path,
    identity: &LinkStrategyIdentity,
) -> Result<Vec<MaterializedBindingRow>, Refusal> {
    let file = File::open(path).map_err(|error| {
        link_io_refusal("Failed to open entity link materialized rows", path, error)
    })?;
    let mut reader = ReaderBuilder::new().has_headers(true).from_reader(file);
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
    let mut expected = vec![
        artifact.shared_run_artifact.clone(),
        artifact.shared_solve_artifact.clone(),
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

fn hash_file(path: &Path, label: &'static str) -> Result<String, Refusal> {
    let bytes = fs::read(path).map_err(|error| {
        link_io_refusal(format!("Failed to read entity link {label}"), path, error)
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn read_observation_surface_bindings(
    path: &Path,
) -> Result<Vec<EntityLinkObservationSurfaceBinding>, Refusal> {
    let file = File::open(path).map_err(|error| {
        link_io_refusal(
            "Failed to open entity link observation/surface bindings",
            path,
            error,
        )
    })?;
    let reader = BufReader::new(file);
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

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), Refusal> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        link_artifact_refusal(
            "Failed to serialize entity link artifact",
            json!({
                "stage": "link",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            link_io_refusal(
                "Failed to create entity link artifact directory",
                parent,
                error,
            )
        })?;
    }
    fs::write(path, bytes)
        .map_err(|error| link_io_refusal("Failed to write entity link artifact", path, error))
}

fn write_jsonl_file<T: Serialize>(path: &Path, values: &[T]) -> Result<(), Refusal> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            link_io_refusal(
                "Failed to create entity link artifact directory",
                parent,
                error,
            )
        })?;
    }
    let mut file = File::create(path).map_err(|error| {
        link_io_refusal(
            "Failed to create entity link observation/surface bindings",
            path,
            error,
        )
    })?;
    for value in values {
        serde_json::to_writer(&mut file, value).map_err(|error| {
            link_artifact_refusal(
                "Failed to serialize entity link observation/surface binding",
                json!({
                    "stage": "link",
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        file.write_all(b"\n").map_err(|error| {
            link_io_refusal(
                "Failed to write entity link observation/surface binding",
                path,
                error,
            )
        })?;
    }
    Ok(())
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
    use crate::entity::run::RUN_CACHE_EXECUTION_RECEIPT_PATH;
    use std::path::PathBuf;

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
