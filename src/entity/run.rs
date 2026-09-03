#![forbid(unsafe_code)]

//! Artifact-backed `canon entity run` orchestration.
//!
//! This runner keeps the happy path local and resumable by persisting every
//! stage artifact under the caller's work directory and emitting a compact run
//! artifact that records the chained stage hashes.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1,
        CANON_ENTITY_INDEX_VERSION_V1, CANON_ENTITY_PREPARE_VERSION_V1,
        CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactHeader,
        EntityArtifactMetadata, EntityArtifactReference, EntityArtifactReferenceV1,
        EntityArtifactStageV1, EntityDeterministicSummary, EntityStrategyReference,
        block::{
            BlockCandidateBudgetConfig, BlockCandidateBudgetObservation,
            BlockCandidateGenerationRequest, BlockCandidateHit, BlockCandidateOperator,
            BlockCandidateRecord, EntityBlockStageOutput, EntityBlockStageRequest,
            EntityNativeBlockBudgetRefusalProof, EntityNativeBlockScaleReport,
            ExactBucketBlockRequest, ExactBucketSurface, RareTokenOverlapBlockOperator,
            default_block_candidate_operators, emit_exact_bucket_hyperedges,
            generate_block_candidates, load_block_runtime_config,
            native_block_budget_refusal_proof, native_block_scale_report,
        },
        block_artifact::{
            BlockCandidateArtifact, BlockCandidateArtifactRequest, ExactBucketAssertion,
            ExactBucketProfile, ExactBucketUpstream, block_candidate_record_cmp,
            build_block_candidate_artifact_contract, validate_block_candidate_artifact_contract,
            validate_block_candidate_artifact_envelope_contract,
            validate_block_candidate_payload_hashes,
        },
        edge::{
            EdgeEvidenceHit, EdgeEvidenceRecord, EntityEvidenceStageOutput,
            EntityEvidenceStageRequest, build_edge_evidence_record,
        },
        edge_artifact::{
            EdgeEvidenceArtifact, EdgeEvidenceArtifactRequest,
            build_edge_evidence_artifact_from_validated_block_contract,
            validate_edge_evidence_artifact_contract,
            validate_edge_evidence_artifact_envelope_contract,
            validate_edge_evidence_payload_hashes,
        },
        error::EntityRefusalKind,
        evidence::{
            ExactViewSupportRequest, StringSimilaritySupportRequest, exact_view_support_hit,
            string_similarity_support_hit,
        },
        evidence_ir::{CANON_EVIDENCE_VERSION, canonical_bundle_bytes},
        graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex, EntityNgramSurface},
        index::{
            EntityIndexArtifact, EntityIndexBuildRequest, EntityIndexCacheMode,
            EntityIndexCacheStatus, EntityNativeIndexScaleReport, native_index_scale_report,
            run_index_build_v1_with_cache_mode,
        },
        index_io::{
            CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION, EntityIndexCacheReceipt,
            INDEX_CACHE_RECEIPT_FILE, preflight_index_cache_entry_paths,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
        prepare::{
            DEFAULT_PREPARE_ROWS_PER_CHUNK, LoadedPrepareProfile, PrepareRunArtifact,
            PrepareRunRequest, PreparedExactLookupStatus, PreparedInputObservation,
            PreparedSurfaceRecord, load_prepare_profile_with_hash,
            prepare_contract_for_loaded_profile, project_prepare_path,
            run_prepare_v1_with_target_rows_per_chunk,
        },
        profile::{EntityOperatorSpec, EntityProfileDocument},
        publication::{
            CANON_ENTITY_STAGE_PUBLICATION_VERSION, EntityPublicationError,
            EntityPublicationErrorKind, EntityPublicationFileInput, EntityPublicationOutcome,
            EntityPublicationReceipt, EntityPublicationRequest, EntityPublicationUpstreamRef,
            open_current_stream_generation, publish_stream_patch,
        },
        record_link::{
            ASSIGNMENT_ALIGNMENT_PATH, ASSIGNMENT_ALIGNMENT_VERSION,
            AssignmentAlignmentDecisionKind, AssignmentAlignmentPolicy,
            RECORD_LINK_CANDIDATE_SET_VERSION, RecordLinkCandidateConfig,
            RecordLinkCandidateRequest, RecordLinkCandidateSet, RecordLinkCoreError,
            RecordLinkEdgeHit, RecordLinkEvidenceRequest, RecordLinkFeaturePolicy,
            RecordLinkInputSet, RecordLinkLoadRequest, RecordLinkSurfaceBindingInput,
            bind_record_link_surfaces, build_record_link_evidence,
            canonical_assignment_alignment_bytes, canonical_record_link_candidate_set_bytes,
            generate_record_link_candidates, load_record_link_inputs,
            validate_record_link_blocking_policy, validate_record_link_candidate_set,
        },
        schema::{
            entity_v1_artifact_reference, entity_v1_contract_for_stage,
            entity_v1_lifecycle_metadata_from_source, finalize_entity_v1_self_hash,
            sort_entity_v1_upstream_references, validate_artifact_v1_core_contract,
            validate_entity_v1_self_hash,
        },
        score::{ScoreLane, ScoreUnits},
        solve::{
            EntitySolveStageOutput, EntitySolveStageRequest, SolveAliasProposalSurface,
            SolveAliasProposalSurfaceStatus, SolveArtifact, SolveArtifactRequest,
            SolveDiagnosticsReport, SolveReconciliationConfig, SolveSurfaceProvenance,
            build_solve_artifact_contract_with_alias_proposals, build_solve_diagnostics,
            validate_solve_artifact_contract,
        },
        tfidf_evidence::{TfidfCosineSupportRequest, tfidf_cosine_support_hit},
    },
    namekit::{
        ngram::NgramConfig,
        similarity::SimilarityMetric,
        tfidf::{SparseTfidfModel, TfidfInputSurface},
    },
    witness,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

#[path = "link.rs"]
pub mod link;

const PREPARE_ARTIFACT_PATH: &str = "prepare/prepare.json";
const BLOCK_ARTIFACT_PATH: &str = "block/block.json";
const BLOCK_CANDIDATES_PATH: &str = "block/candidates.jsonl";
const BLOCK_DIAGNOSTICS_PATH: &str = "block/diagnostics.json";
const BLOCK_EXACT_BUCKETS_PATH: &str = "block/exact_buckets.jsonl";
const RECORD_LINK_CANDIDATES_PATH: &str = "block/record_link_candidates.json";
const RECORD_LINK_EVIDENCE_PATH: &str = "evidence/record_link_evidence.json";
const EDGE_ARTIFACT_PATH: &str = "evidence/evidence.json";
const EDGE_RECORDS_PATH: &str = "evidence/evidence.jsonl";
const SOLVE_ARTIFACT_PATH: &str = "solve/solve.json";
const DECISION_LEDGER_PATH: &str = "solve/decision_ledger.jsonl";
const RUN_MANIFEST_PATH: &str = "run/manifest.json";
const RUN_ARTIFACT_PATH: &str = "run/run.json";
const LINK_MATERIALIZED_ROWS_PUBLICATION_PATH: &str = "link/combined_rows.csv";
const LINK_ASSIGNMENT_ALIGNMENT_PUBLICATION_PATH: &str = "link/assignment_alignment.json";
const LINK_OBSERVATION_SURFACE_BINDINGS_PUBLICATION_PATH: &str =
    "link/observation_surface_bindings.jsonl";
const LINK_ARTIFACT_PUBLICATION_PATH: &str = "link/link.json";
pub const RUN_CACHE_EXECUTION_RECEIPT_PATH: &str = "run/cache_execution_receipt.json";
pub const ENTITY_RUN_PUBLICATION_STREAM_ID: &str = "entity-run-stage-set";
const RUN_PUBLICATION_STREAM_ID: &str = ENTITY_RUN_PUBLICATION_STREAM_ID;
const RUN_PUBLICATION_REQUEST_VERSION: &str = "canon_entity_run_publication_request.v1";

pub fn read_entity_run_committed_publication_logical_bytes(
    work_dir: &Path,
    logical_path: &str,
) -> Result<Option<Vec<u8>>, Refusal> {
    read_entity_run_committed_publication_logical_bytes_inner(work_dir, logical_path, None, None)
}

pub fn read_entity_run_committed_publication_stable_path_bytes(
    work_dir: &Path,
    path: &Path,
) -> Result<Option<Vec<u8>>, Refusal> {
    let logical_path = entity_run_publication_logical_path_for_stable_path(work_dir, path)?;
    read_entity_run_committed_publication_logical_bytes(work_dir, logical_path)
}

pub fn publish_entity_run_link_publication_patch(
    work_dir: &Path,
    expected_parent_generation_id: &str,
    upstream_artifacts: Vec<EntityArtifactReference>,
    files: Vec<EntityPublicationFileInput>,
) -> Result<EntityRunPublicationResult, Refusal> {
    let current = open_current_stream_generation(work_dir, ENTITY_RUN_PUBLICATION_STREAM_ID)
        .map_err(|error| {
            publication_refusal_with_next_command(
                error,
                Some("Use canon entity link to regenerate link/link.json".to_string()),
            )
        })?;
    if current.generation_id != expected_parent_generation_id {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity link publication parent no longer matches the validated run generation",
            json!({
                "stage": "link",
                "publication_stage": "entity_run_stage_set",
                "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                "expected_generation_id": expected_parent_generation_id,
                "actual_generation_id": current.generation_id,
                "committed": true,
                "writes_performed": false
            }),
            Some("Use canon entity link to regenerate link/link.json".to_string()),
        ));
    }
    for logical_path in [RUN_ARTIFACT_PATH, SOLVE_ARTIFACT_PATH] {
        if current.read_logical_file(logical_path).is_none() {
            return Err(EntityRefusalKind::ArtifactContract.to_refusal(
                "Entity link publication parent is missing a required run-stage artifact",
                json!({
                    "stage": "link",
                    "publication_stage": "entity_run_stage_set",
                    "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                    "generation_id": current.generation_id,
                    "logical_path": logical_path,
                    "writes_performed": false
                }),
                Some("Use canon entity link to regenerate link/link.json".to_string()),
            ));
        }
    }
    let mut publication_upstreams = upstream_artifacts
        .into_iter()
        .map(|reference| publication_upstream_ref(reference.version, reference.content_hash))
        .collect::<Vec<_>>();
    let parent_generation_id = current.generation_id.clone();
    publication_upstreams.push(publication_upstream_ref(
        CANON_ENTITY_STAGE_PUBLICATION_VERSION,
        parent_generation_id.clone(),
    ));
    let publication = publish_stage_generation_at_work_dir(
        work_dir,
        Some("Use canon entity link to regenerate link/link.json".to_string()),
        Some(parent_generation_id),
        EntityRunPublicationCacheInput {
            mode: &current.manifest.cache_mode,
            status: &current.manifest.cache_status,
            receipt_hash: &current.manifest.cache_receipt_hash,
        },
        publication_upstreams,
        &files,
    )?;
    mirror_publication_files_at_work_dir(
        work_dir,
        Some("Use canon entity link to regenerate link/link.json".to_string()),
        &publication,
        &files,
    )?;
    Ok(publication)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityRunRequest<'a> {
    pub rows: &'a Path,
    pub profile: &'a str,
    pub strategy: &'a Path,
    pub registry: &'a Path,
    pub work_dir: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityRunBatchConfig {
    pub target_rows_per_batch: u64,
}

impl EntityRunBatchConfig {
    pub const fn new(target_rows_per_batch: u64) -> Self {
        Self {
            target_rows_per_batch,
        }
    }
}

impl Default for EntityRunBatchConfig {
    fn default() -> Self {
        Self {
            target_rows_per_batch: DEFAULT_PREPARE_ROWS_PER_CHUNK,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRunResult {
    pub artifact: EntityRunArtifact,
    pub artifact_value: Value,
    pub candidate_pairs: u64,
    pub publication: EntityRunPublicationResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRunPublicationResult {
    pub stream_id: String,
    pub generation_id: String,
    pub outcome: String,
    pub writes_performed: bool,
    pub committed: Option<bool>,
    pub manifest_path: String,
    pub commit_marker_path: String,
    pub object_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRunArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub stage_artifacts: Vec<EntityRunStageArtifact>,
    pub work_dir: EntityRunWorkDirLayout,
    pub next_commands: EntityRunNextCommands,
    #[serde(default)]
    pub orchestration: EntityRunOrchestration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRunStageArtifact {
    pub stage: String,
    pub version: String,
    pub path: String,
    pub artifact_content_hash: String,
    #[serde(default)]
    pub upstream_artifacts: Vec<EntityArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRunWorkDirLayout {
    pub prepare_artifact_path: String,
    pub surfaces_path: String,
    pub index_artifact_path: String,
    pub block_artifact_path: String,
    pub candidate_records_path: String,
    pub candidate_diagnostics_path: String,
    pub exact_bucket_assertions_path: String,
    #[serde(rename = "evidence_artifact_path")]
    pub edge_artifact_path: String,
    #[serde(rename = "evidence_records_path")]
    pub edge_records_path: String,
    pub solve_artifact_path: String,
    pub decision_ledger_path: String,
    pub run_artifact_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRunNextCommands {
    pub resume: String,
    pub review_export: String,
    pub audit: String,
    pub promote: String,
    pub apply: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityRunOrchestration {
    pub stage_order: Vec<String>,
    pub profile_firewall: EntityRunProfileFirewall,
    pub handoff_steps: Vec<EntityRunHandoffStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityRunProfileFirewall {
    pub profile_id: String,
    pub profile_version: String,
    pub identity_semantics: String,
    pub canonical_type: String,
    pub registry_id: String,
    pub registry_version: String,
    pub registry_snapshot_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar_snapshot_hash: Option<String>,
    pub strategy_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityRunHandoffStep {
    pub stage: String,
    pub command: String,
    pub input_artifact_path: String,
    #[serde(default)]
    pub input_artifacts: Vec<EntityArtifactReference>,
    #[serde(default)]
    pub required_paths: Vec<String>,
    #[serde(default)]
    pub output_paths: Vec<String>,
    #[serde(default)]
    pub required_prior_stages: Vec<String>,
    pub requires_audit: bool,
    pub enforces_profile_firewall: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BaseStrategyReference {
    id: String,
    version: String,
    content_hash: String,
    record_link: Option<RecordLinkRuntimeConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordLinkRuntimeConfig {
    input_paths: Vec<PathBuf>,
    candidate_config: RecordLinkCandidateConfig,
    assignment_alignment: AssignmentAlignmentPolicy,
    assignment_hint_score_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordLinkBlockRun {
    input_set: RecordLinkInputSet,
    candidate_set: RecordLinkCandidateSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordLinkEvidenceRun {
    evidence_bytes: Vec<u8>,
    alignment_bytes: Vec<u8>,
    edge_hits: BTreeMap<(String, String), Vec<RecordLinkEdgeHit>>,
}

#[derive(Debug, Deserialize)]
struct RecordLinkStrategySection {
    inputs: Vec<RecordLinkStrategyInput>,
    operator_id: Option<String>,
    #[serde(default)]
    blocking: Option<crate::entity::record_link::RecordLinkBlockingPolicy>,
    max_candidates_per_record: usize,
    max_candidate_pairs: usize,
    max_pair_comparisons: usize,
    #[serde(default)]
    require_unique_best_per_record: Option<bool>,
    feature_policies: Vec<RecordLinkFeaturePolicy>,
    assignment_alignment: AssignmentAlignmentPolicy,
    assignment_hint_score_units: u64,
}

#[derive(Debug, Deserialize)]
struct RecordLinkStrategyInput {
    path: PathBuf,
}

pub fn run_entity_workbench(request: EntityRunRequest<'_>) -> Result<EntityRunResult, Refusal> {
    run_entity_workbench_with_batching(request, EntityRunBatchConfig::default())
}

pub fn run_entity_workbench_with_cache_mode(
    request: EntityRunRequest<'_>,
    cache_mode: EntityIndexCacheMode,
) -> Result<EntityRunResult, Refusal> {
    run_entity_workbench_with_batching_and_cache_mode(
        request,
        EntityRunBatchConfig::default(),
        cache_mode,
    )
}

pub fn run_entity_workbench_with_batching(
    request: EntityRunRequest<'_>,
    batch_config: EntityRunBatchConfig,
) -> Result<EntityRunResult, Refusal> {
    run_entity_workbench_with_batching_and_cache_mode(
        request,
        batch_config,
        EntityIndexCacheMode::Enabled,
    )
}

pub fn run_entity_workbench_with_batching_and_cache_mode(
    request: EntityRunRequest<'_>,
    batch_config: EntityRunBatchConfig,
    cache_mode: EntityIndexCacheMode,
) -> Result<EntityRunResult, Refusal> {
    preflight_index_cache_entry_paths(request.work_dir)
        .map_err(|refusal| with_run_context(refusal, "index", request))?;
    let base_strategy = load_base_strategy_reference(request)
        .map_err(|refusal| with_run_context(refusal, "strategy", request))?;
    let prepare_value = run_prepare_v1_with_target_rows_per_chunk(
        PrepareRunRequest {
            rows: request.rows,
            profile: request.profile,
            registry: request.registry,
            work_dir: request.work_dir,
        },
        batch_config.target_rows_per_batch,
    )
    .map_err(|refusal| with_run_context(refusal, "prepare", request))?;
    let prepare = deserialize_artifact_value::<PrepareRunArtifact>(
        &prepare_value,
        "prepare",
        "entity prepare v1 artifact",
    )?;
    let surfaces = read_surfaces(request.work_dir, &prepare)
        .map_err(|refusal| with_run_context(refusal, "prepare", request))?;
    let prepare_header = prepare_header(&prepare);

    let index = build_and_write_index(
        request,
        &base_strategy,
        &prepare_header,
        &surfaces,
        cache_mode,
    )
    .map_err(|refusal| with_run_context(refusal, "index", request))?;
    let block = build_and_write_block(request, &base_strategy, &index, &surfaces, false)
        .map_err(|refusal| with_run_context(refusal, "block", request))?;
    let (edge, edge_value, edge_records, edge_files) =
        build_and_write_edge(request, &base_strategy, &block, &surfaces, false)
            .map_err(|refusal| with_run_context(refusal, "evidence", request))?;
    let solve_input = SolveStageInput {
        edge: &edge,
        edge_value: &edge_value,
        edge_records: &edge_records,
        exact_buckets: &block.exact_buckets,
        surfaces: &surfaces,
    };
    let (solve, solve_value, solve_files) =
        build_and_write_solve(request, &base_strategy, solve_input, false)
            .map_err(|refusal| with_run_context(refusal, "solve", request))?;

    let (artifact, artifact_value) = run_artifact(
        request,
        &base_strategy,
        &prepare,
        &prepare_value,
        &surfaces,
        &index,
        &block.artifact,
        &block.artifact_value,
        &edge,
        &edge_value,
        &solve,
        &solve_value,
    )?;
    let mut publication_files = Vec::new();
    publication_files.extend(block.publication_files.clone());
    publication_files.extend(edge_files);
    publication_files.extend(solve_files);
    publication_files.extend(run_publication_files(&artifact, &artifact_value)?);
    let publication = publish_run_stage_generation(
        request,
        &index,
        &artifact.stage_artifacts,
        &publication_files,
    )
    .map_err(|refusal| with_run_context(refusal, "run", request))?;
    mirror_publication_files(request, &publication, &publication_files)
        .map_err(|refusal| with_run_context(refusal, "run", request))?;

    Ok(EntityRunResult {
        candidate_pairs: block.artifact.summary.counts["candidate_pairs"],
        artifact,
        artifact_value,
        publication,
    })
}

pub fn run_entity_block_stage(
    request: EntityBlockStageRequest<'_>,
) -> Result<EntityBlockStageOutput, Refusal> {
    run_entity_block_stage_with_batching(request, EntityRunBatchConfig::default())
}

pub fn run_entity_block_stage_with_batching(
    request: EntityBlockStageRequest<'_>,
    batch_config: EntityRunBatchConfig,
) -> Result<EntityBlockStageOutput, Refusal> {
    let run_request = block_stage_request_to_run_request(request);
    let base_strategy = load_base_strategy_reference(run_request)
        .map_err(|refusal| with_run_context(refusal, "strategy", run_request))?;
    let prepare_value = run_prepare_v1_with_target_rows_per_chunk(
        PrepareRunRequest {
            rows: run_request.rows,
            profile: run_request.profile,
            registry: run_request.registry,
            work_dir: run_request.work_dir,
        },
        batch_config.target_rows_per_batch,
    )
    .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let prepare = deserialize_artifact_value::<PrepareRunArtifact>(
        &prepare_value,
        "prepare",
        "entity prepare v1 artifact",
    )?;
    let surfaces = read_surfaces(run_request.work_dir, &prepare)
        .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let prepare_header = prepare_header(&prepare);
    let index = build_and_write_index(
        run_request,
        &base_strategy,
        &prepare_header,
        &surfaces,
        EntityIndexCacheMode::Enabled,
    )
    .map_err(|refusal| with_run_context(refusal, "index", run_request))?;
    let block = build_and_write_block(run_request, &base_strategy, &index, &surfaces, true)
        .map_err(|refusal| with_run_context(refusal, "block", run_request))?;

    Ok(EntityBlockStageOutput {
        artifact: block.artifact,
        candidates: block.candidates,
        exact_buckets: block.exact_buckets,
    })
}

pub fn run_entity_evidence_stage(
    request: EntityEvidenceStageRequest<'_>,
) -> Result<EntityEvidenceStageOutput, Refusal> {
    run_entity_evidence_stage_with_batching(request, EntityRunBatchConfig::default())
}

pub fn run_entity_evidence_stage_with_batching(
    request: EntityEvidenceStageRequest<'_>,
    batch_config: EntityRunBatchConfig,
) -> Result<EntityEvidenceStageOutput, Refusal> {
    let run_request = evidence_stage_request_to_run_request(request);
    let base_strategy = load_base_strategy_reference(run_request)
        .map_err(|refusal| with_run_context(refusal, "strategy", run_request))?;
    let prepare_value = run_prepare_v1_with_target_rows_per_chunk(
        PrepareRunRequest {
            rows: run_request.rows,
            profile: run_request.profile,
            registry: run_request.registry,
            work_dir: run_request.work_dir,
        },
        batch_config.target_rows_per_batch,
    )
    .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let prepare = deserialize_artifact_value::<PrepareRunArtifact>(
        &prepare_value,
        "prepare",
        "entity prepare v1 artifact",
    )?;
    let surfaces = read_surfaces(run_request.work_dir, &prepare)
        .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let prepare_header = prepare_header(&prepare);
    let index = build_and_write_index(
        run_request,
        &base_strategy,
        &prepare_header,
        &surfaces,
        EntityIndexCacheMode::Enabled,
    )
    .map_err(|refusal| with_run_context(refusal, "index", run_request))?;
    let block = read_block_stage_from_artifact(
        run_request,
        &base_strategy,
        &index,
        &surfaces,
        request.candidates,
    )
    .map_err(|refusal| with_run_context(refusal, "block", run_request))?;
    let (artifact, _artifact_value, records, _edge_files) =
        build_and_write_edge(run_request, &base_strategy, &block, &surfaces, true)
            .map_err(|refusal| with_run_context(refusal, "evidence", run_request))?;

    Ok(EntityEvidenceStageOutput {
        artifact,
        records,
        candidate_records: block.candidates,
        exact_buckets: block.exact_buckets,
    })
}

pub fn run_entity_solve_stage(
    request: EntitySolveStageRequest<'_>,
) -> Result<EntitySolveStageOutput, Refusal> {
    run_entity_solve_stage_with_batching(request, EntityRunBatchConfig::default())
}

pub fn run_entity_solve_stage_with_batching(
    request: EntitySolveStageRequest<'_>,
    batch_config: EntityRunBatchConfig,
) -> Result<EntitySolveStageOutput, Refusal> {
    let run_request = solve_stage_request_to_run_request(request);
    let base_strategy = load_base_strategy_reference(run_request)
        .map_err(|refusal| with_run_context(refusal, "strategy", run_request))?;
    let prepare_value = run_prepare_v1_with_target_rows_per_chunk(
        PrepareRunRequest {
            rows: run_request.rows,
            profile: run_request.profile,
            registry: run_request.registry,
            work_dir: run_request.work_dir,
        },
        batch_config.target_rows_per_batch,
    )
    .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let prepare = deserialize_artifact_value::<PrepareRunArtifact>(
        &prepare_value,
        "prepare",
        "entity prepare v1 artifact",
    )?;
    let surfaces = read_surfaces(run_request.work_dir, &prepare)
        .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let (edge, edge_value, edge_records, exact_buckets) =
        read_edge_stage_from_artifact(run_request, &base_strategy, &prepare, request.evidence)
            .map_err(|refusal| with_run_context(refusal, "evidence", run_request))?;
    let solve_input = SolveStageInput {
        edge: &edge,
        edge_value: &edge_value,
        edge_records: &edge_records,
        exact_buckets: &exact_buckets,
        surfaces: &surfaces,
    };
    let (solve, _solve_value, _solve_files) =
        build_and_write_solve(run_request, &base_strategy, solve_input, true)
            .map_err(|refusal| with_run_context(refusal, "solve", run_request))?;

    Ok(EntitySolveStageOutput { artifact: solve })
}

pub fn render_run_summary(artifact: &EntityRunArtifact) -> String {
    let counts = &artifact.summary.counts;
    let labels = &artifact.summary.labels;
    format!(
        "{} profile={} registry={}@{} rows={} surfaces={} exact_resolved={} candidate_pairs={} evidence_records={} entities={} review_groups={} run_artifact={}",
        artifact.version,
        labels.get("profile_id").map_or("", String::as_str),
        labels.get("registry_id").map_or("", String::as_str),
        labels.get("registry_version").map_or("", String::as_str),
        count(counts, "row_count"),
        count(counts, "prepared_surfaces"),
        count(counts, "exact_resolved_surfaces"),
        count(counts, "candidate_pairs"),
        count(counts, "evidence_records"),
        count(counts, "solved_entities"),
        count(counts, "review_group_count"),
        artifact.work_dir.run_artifact_path
    )
}

pub const CANON_ENTITY_NATIVE_SCALE_PROOF_VERSION: &str = "canon_entity_native_scale_proof.v0";
pub const CANON_ENTITY_NATIVE_SCALE_GENERATOR_VERSION: &str =
    "canon_entity_native_scale_generator.v0";

const NATIVE_SCALE_PROFILE_ID: &str = "native_scale_offline";
const NATIVE_SCALE_CORE_VIEW: &str = "tenant_core";
const NATIVE_SCALE_BLOCK_OPERATOR_ID: &str = "rare_token_overlap:native_scale";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleProofConfig {
    pub observation_count: u64,
    pub source_count: u32,
    pub entity_count: u32,
    pub variants_per_entity: u32,
    pub rows_per_chunk: u64,
    pub posting_common_posting_limit: usize,
    pub ngram_common_posting_limit: usize,
    pub ngram_width: usize,
    pub rare_token_topk: usize,
    pub rare_token_candidate_cap: usize,
    pub max_candidates_per_surface: u64,
    pub max_candidates_per_operator: u64,
    pub max_candidates_per_run: u64,
    pub edge_sample_limit: usize,
}

impl NativeScaleProofConfig {
    pub const fn offline_500k() -> Self {
        Self {
            observation_count: 500_000,
            source_count: 5,
            entity_count: 512,
            variants_per_entity: 2,
            rows_per_chunk: 16_384,
            posting_common_posting_limit: 64,
            ngram_common_posting_limit: 64,
            ngram_width: 3,
            rare_token_topk: 8,
            rare_token_candidate_cap: 8,
            max_candidates_per_surface: 16,
            max_candidates_per_operator: 100_000,
            max_candidates_per_run: 100_000,
            edge_sample_limit: 512,
        }
    }

    pub const fn smoke() -> Self {
        Self {
            observation_count: 50_000,
            source_count: 4,
            entity_count: 128,
            variants_per_entity: 2,
            rows_per_chunk: 8_192,
            posting_common_posting_limit: 64,
            ngram_common_posting_limit: 64,
            ngram_width: 3,
            rare_token_topk: 8,
            rare_token_candidate_cap: 8,
            max_candidates_per_surface: 16,
            max_candidates_per_operator: 25_000,
            max_candidates_per_run: 25_000,
            edge_sample_limit: 128,
        }
    }

    pub const fn with_observation_count(mut self, observation_count: u64) -> Self {
        self.observation_count = observation_count;
        self
    }
}

impl Default for NativeScaleProofConfig {
    fn default() -> Self {
        Self::offline_500k()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleProof {
    pub version: String,
    pub artifact_content_hash: String,
    pub generator: NativeScaleGenerator,
    pub config: NativeScaleProofConfig,
    pub offline: NativeScaleOfflineProof,
    pub intake: NativeScaleIntakeReport,
    pub cache: NativeScaleCacheProof,
    pub index: EntityNativeIndexScaleReport,
    pub block: EntityNativeBlockScaleReport,
    pub budget_refusal: EntityNativeBlockBudgetRefusalProof,
    pub edge_record_count: u64,
    pub solve: NativeScaleSolveReport,
    pub artifact_publication: NativeScaleArtifactPublication,
    pub stage_metrics: Vec<NativeScaleStageMetric>,
    pub reproducible_commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleGenerator {
    pub id: String,
    pub version: String,
    pub deterministic: bool,
    pub input_content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleOfflineProof {
    pub network_required: bool,
    pub python_required: bool,
    pub adapter_required: bool,
    pub input_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleIntakeReport {
    pub observation_count: u64,
    pub source_count: u64,
    pub entity_count: u64,
    pub unique_surface_count: u64,
    pub duplicate_observation_count: u64,
    pub chunk_count: u64,
    pub rows_per_chunk: u64,
    pub max_live_surface_records: u64,
    pub unicode_observation_count: u64,
    pub sparse_anchor_observation_count: u64,
    pub hard_negative_observation_count: u64,
    pub multisource_entity_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleCacheProof {
    pub cold_cache_status: EntityIndexCacheStatus,
    pub warm_cache_status: EntityIndexCacheStatus,
    pub changed_input_cache_status: EntityIndexCacheStatus,
    pub cold_cache_key_hash: String,
    pub changed_input_cache_key_hash: String,
    pub changed_fields: Vec<String>,
    pub invalidated_layers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleSolveReport {
    pub graph_surface_node_count: u64,
    pub support_edge_count: u64,
    pub hard_cannot_link_edge_count: u64,
    pub solved_component_count: u64,
    pub review_group_count: u64,
    pub diagnostics: SolveDiagnosticsReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleArtifactPublication {
    pub artifact_count: u64,
    pub artifact_bytes: u64,
    pub disk_write_bytes: u64,
    pub deterministic_content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeScaleStageMetric {
    pub stage: String,
    pub input_records: u64,
    pub output_records: u64,
    pub artifact_bytes: u64,
    pub disk_write_bytes: u64,
    pub wall_time_ms: Option<u64>,
    pub peak_rss_bytes: Option<u64>,
}

pub fn prove_native_engine_scale_offline(
    config: NativeScaleProofConfig,
) -> Result<NativeScaleProof, Refusal> {
    validate_native_scale_config(&config)?;
    let input = stream_native_scale_observations(&config);
    let posting_surfaces = native_posting_surfaces(&input.surfaces);
    let ngram_surfaces = native_ngram_surfaces(&input.surfaces);

    let postings = EntityPostingIndex::build(
        &posting_surfaces,
        EntityPostingBuildConfig {
            common_posting_limit: config.posting_common_posting_limit,
        },
    )
    .map_err(|error| {
        native_scale_artifact_refusal(
            "Failed to build native scale postings index",
            json!({
                "stage": "index",
                "error": format!("{error:?}"),
                "writes_performed": false
            }),
        )
    })?;
    let ngrams = EntityNgramIndex::build(
        &ngram_surfaces,
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(config.ngram_width).ok_or_else(|| {
                native_scale_artifact_refusal(
                    "Native scale proof requires a positive n-gram width",
                    json!({
                        "stage": "index",
                        "field": "ngram_width",
                        "actual": config.ngram_width,
                        "writes_performed": false
                    }),
                )
            })?,
            common_posting_limit: config.ngram_common_posting_limit,
        },
    )
    .map_err(|error| {
        native_scale_artifact_refusal(
            "Failed to build native scale n-gram index",
            json!({
                "stage": "index",
                "error": format!("{error:?}"),
                "writes_performed": false
            }),
        )
    })?;

    let cache_key_material = native_cache_key_material(&config, &input.generator_hash);
    let index_artifact_bytes = serialized_len(&(&postings.diagnostics, &ngrams.diagnostics))?;
    let index = native_index_scale_report(
        &postings.diagnostics,
        &ngrams.diagnostics,
        EntityIndexCacheStatus::Rebuilt,
        index_artifact_bytes,
        &cache_key_material,
    );

    let block_result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: NATIVE_SCALE_PROFILE_ID.to_string(),
        posting_index: &postings,
        ngram_index: Some(&ngrams),
        budget_config: BlockCandidateBudgetConfig::new(
            config.max_candidates_per_surface,
            config.max_candidates_per_operator,
            config.max_candidates_per_run,
        ),
        operators: vec![BlockCandidateOperator::RareTokenOverlap(
            RareTokenOverlapBlockOperator::new(
                NATIVE_SCALE_BLOCK_OPERATOR_ID,
                NATIVE_SCALE_CORE_VIEW,
            )
            .with_min_idf_units(0)
            .with_topk(config.rare_token_topk, config.rare_token_candidate_cap)
            .with_max_posting_size(config.posting_common_posting_limit),
        )],
    })?;
    let block = native_block_scale_report(&block_result.diagnostics);

    let budget_refusal = native_block_budget_refusal_proof(
        &BlockCandidateBudgetConfig::new(1, 1, 1),
        &[BlockCandidateBudgetObservation::new(
            "native:surface:budget",
            NATIVE_SCALE_BLOCK_OPERATOR_ID,
            2,
            0,
        )],
    )?;

    let edge_records = native_edge_records(&block_result.candidates, config.edge_sample_limit)?;
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: edge_records.clone(),
        exact_bucket_assertions: Vec::new(),
        incumbent_ids: Vec::new(),
    })?;
    let solve_provenance = native_solve_provenance(&input.surfaces);
    let solve_diagnostics = build_solve_diagnostics(
        &graph,
        SolveReconciliationConfig::escrow_only(ScoreUnits::saturating_from_units(1)),
        &solve_provenance,
    );
    let solve = NativeScaleSolveReport {
        graph_surface_node_count: graph.diagnostics.surface_node_count,
        support_edge_count: graph.diagnostics.support_edge_count,
        hard_cannot_link_edge_count: graph.diagnostics.hard_cannot_link_edge_count,
        solved_component_count: solve_diagnostics
            .summary
            .get("component_count")
            .copied()
            .unwrap_or_default(),
        review_group_count: solve_diagnostics.review_group_seeds.len() as u64,
        diagnostics: solve_diagnostics,
    };

    let changed_cache_key_material = format!("{cache_key_material}:changed_input");
    let cache = NativeScaleCacheProof {
        cold_cache_status: EntityIndexCacheStatus::Rebuilt,
        warm_cache_status: EntityIndexCacheStatus::Hit,
        changed_input_cache_status: EntityIndexCacheStatus::Miss,
        cold_cache_key_hash: witness::hash_bytes(cache_key_material.as_bytes()),
        changed_input_cache_key_hash: witness::hash_bytes(changed_cache_key_material.as_bytes()),
        changed_fields: vec!["input_hash".to_string()],
        invalidated_layers: vec!["ngram_postings".to_string()],
    };

    let mut proof = NativeScaleProof {
        version: CANON_ENTITY_NATIVE_SCALE_PROOF_VERSION.to_string(),
        artifact_content_hash: String::new(),
        generator: NativeScaleGenerator {
            id: "native_scale_generator".to_string(),
            version: CANON_ENTITY_NATIVE_SCALE_GENERATOR_VERSION.to_string(),
            deterministic: true,
            input_content_hash: input.generator_hash.clone(),
        },
        config,
        offline: NativeScaleOfflineProof {
            network_required: false,
            python_required: false,
            adapter_required: false,
            input_source: "deterministic_native_generator".to_string(),
        },
        intake: input.report,
        cache,
        index,
        block,
        budget_refusal,
        edge_record_count: edge_records.len() as u64,
        solve,
        artifact_publication: NativeScaleArtifactPublication {
            artifact_count: 6,
            artifact_bytes: 0,
            disk_write_bytes: 0,
            deterministic_content_hash: String::new(),
        },
        stage_metrics: Vec::new(),
        reproducible_commands: native_scale_reproducible_commands(),
    };
    proof.stage_metrics = native_scale_stage_metrics(&proof);
    finalize_native_scale_proof(&mut proof)?;
    Ok(proof)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeScaleGeneratedInput {
    surfaces: Vec<NativeScaleSurface>,
    report: NativeScaleIntakeReport,
    generator_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeScaleSurface {
    surface_id: String,
    entity_token: String,
    source_token: String,
    variant_token: String,
    normalized_key: String,
    row_count: u64,
    source_ordinals: BTreeSet<u32>,
}

fn validate_native_scale_config(config: &NativeScaleProofConfig) -> Result<(), Refusal> {
    let invalid_field = if config.observation_count == 0 {
        Some("observation_count")
    } else if config.source_count == 0 {
        Some("source_count")
    } else if config.entity_count == 0 {
        Some("entity_count")
    } else if config.variants_per_entity == 0 {
        Some("variants_per_entity")
    } else if config.rows_per_chunk == 0 {
        Some("rows_per_chunk")
    } else if config.ngram_width == 0 {
        Some("ngram_width")
    } else if config.rare_token_topk == 0 {
        Some("rare_token_topk")
    } else if config.rare_token_candidate_cap == 0 {
        Some("rare_token_candidate_cap")
    } else if config.edge_sample_limit == 0 {
        Some("edge_sample_limit")
    } else {
        None
    };

    if let Some(field) = invalid_field {
        return Err(EntityRefusalKind::InputContract.to_refusal(
            "Native scale proof configuration must use positive limits",
            json!({
                "stage": "native_scale_proof",
                "field": field,
                "writes_performed": false
            }),
            Some(
                "Use NativeScaleProofConfig::offline_500k() or set positive proof limits"
                    .to_string(),
            ),
        ));
    }
    Ok(())
}

fn stream_native_scale_observations(config: &NativeScaleProofConfig) -> NativeScaleGeneratedInput {
    let mut surfaces = BTreeMap::<String, NativeScaleSurface>::new();
    let mut entity_sources = BTreeMap::<u64, BTreeSet<u32>>::new();
    let mut unicode_observation_count = 0_u64;
    let mut sparse_anchor_observation_count = 0_u64;
    let mut hard_negative_observation_count = 0_u64;
    let mut max_live_surface_records = 0_u64;

    let entity_count = u64::from(config.entity_count);
    let source_count = u64::from(config.source_count);
    let variant_count = u64::from(config.variants_per_entity);
    let entity_source_span = entity_count.saturating_mul(source_count).max(1);

    for row_number in 0..config.observation_count {
        let entity_ordinal = row_number % entity_count;
        let source_ordinal = (row_number / entity_count) % source_count;
        let variant_ordinal = (row_number / entity_source_span) % variant_count;
        let surface_id = native_surface_id(entity_ordinal, source_ordinal, variant_ordinal);
        let source_ordinal_u32 = u32::try_from(source_ordinal).unwrap_or(u32::MAX);
        let surface = surfaces.entry(surface_id.clone()).or_insert_with(|| {
            native_scale_surface(entity_ordinal, source_ordinal, variant_ordinal)
        });
        surface.row_count = surface.row_count.saturating_add(1);
        surface.source_ordinals.insert(source_ordinal_u32);
        entity_sources
            .entry(entity_ordinal)
            .or_default()
            .insert(source_ordinal_u32);

        if row_number.is_multiple_of(257) {
            unicode_observation_count = unicode_observation_count.saturating_add(1);
        }
        if row_number.is_multiple_of(19) {
            sparse_anchor_observation_count = sparse_anchor_observation_count.saturating_add(1);
        }
        if row_number.is_multiple_of(997) {
            hard_negative_observation_count = hard_negative_observation_count.saturating_add(1);
        }
        max_live_surface_records =
            max_live_surface_records.max(native_usize_to_u64(surfaces.len()));
    }

    let mut surfaces = surfaces.into_values().collect::<Vec<_>>();
    surfaces.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));
    let unique_surface_count = native_usize_to_u64(surfaces.len());
    let multisource_entity_count = entity_sources
        .values()
        .filter(|sources| sources.len() > 1)
        .count() as u64;
    let report = NativeScaleIntakeReport {
        observation_count: config.observation_count,
        source_count: u64::from(config.source_count),
        entity_count: u64::from(config.entity_count),
        unique_surface_count,
        duplicate_observation_count: config
            .observation_count
            .saturating_sub(unique_surface_count),
        chunk_count: config.observation_count.div_ceil(config.rows_per_chunk),
        rows_per_chunk: config.rows_per_chunk,
        max_live_surface_records,
        unicode_observation_count,
        sparse_anchor_observation_count,
        hard_negative_observation_count,
        multisource_entity_count,
    };
    let generator_hash = native_input_hash(config, &report, &surfaces);
    NativeScaleGeneratedInput {
        surfaces,
        report,
        generator_hash,
    }
}

fn native_scale_surface(
    entity_ordinal: u64,
    source_ordinal: u64,
    variant_ordinal: u64,
) -> NativeScaleSurface {
    let entity_token = format!("entity_{entity_ordinal:05x}");
    let source_token = format!("source_{source_ordinal:02x}");
    let variant_token = format!("variant_{variant_ordinal:02x}");
    let unicode_marker = if entity_ordinal.is_multiple_of(127) && variant_ordinal == 1 {
        " caf\u{e9}_ni\u{f1}o"
    } else {
        ""
    };
    NativeScaleSurface {
        surface_id: native_surface_id(entity_ordinal, source_ordinal, variant_ordinal),
        entity_token: entity_token.clone(),
        source_token: source_token.clone(),
        variant_token: variant_token.clone(),
        normalized_key: format!("{entity_token} {source_token} {variant_token}{unicode_marker}"),
        row_count: 0,
        source_ordinals: BTreeSet::new(),
    }
}

fn native_surface_id(entity_ordinal: u64, source_ordinal: u64, variant_ordinal: u64) -> String {
    format!("native:surface:e{entity_ordinal:05x}:s{source_ordinal:02x}:v{variant_ordinal:02x}")
}

fn native_posting_surfaces(surfaces: &[NativeScaleSurface]) -> Vec<EntityPostingSurface> {
    surfaces
        .iter()
        .map(|surface| {
            EntityPostingSurface::new(surface.surface_id.clone())
                .with_exact_view(NATIVE_SCALE_CORE_VIEW, surface.entity_token.clone())
                .with_exact_view("source_system", surface.source_token.clone())
                .with_tokens([
                    surface.entity_token.clone(),
                    surface.source_token.clone(),
                    surface.variant_token.clone(),
                ])
        })
        .collect()
}

fn native_ngram_surfaces(surfaces: &[NativeScaleSurface]) -> Vec<EntityNgramSurface> {
    surfaces
        .iter()
        .map(|surface| {
            EntityNgramSurface::new(surface.surface_id.clone(), surface.normalized_key.clone())
        })
        .collect()
}

fn native_edge_records(
    candidates: &[crate::entity::block::BlockCandidateRecord],
    edge_sample_limit: usize,
) -> Result<Vec<EdgeEvidenceRecord>, Refusal> {
    candidates
        .iter()
        .take(edge_sample_limit)
        .enumerate()
        .map(|(index, candidate)| {
            let mut hits = vec![EdgeEvidenceHit::new(
                ScoreLane::Support,
                "native_scale",
                NATIVE_SCALE_BLOCK_OPERATOR_ID,
                "candidate_block_support",
                ScoreUnits::saturating_from_units(u64::from(candidate.candidate_score_hint)),
                false,
                "Native scale proof candidate emitted by the block stage",
            )];
            if index % 29 == 0 {
                hits.push(EdgeEvidenceHit::new(
                    ScoreLane::AntiMerge,
                    "native_scale",
                    "hard_negative_control",
                    "offline_hard_negative_anchor",
                    ScoreUnits::MAX,
                    true,
                    "Native scale proof hard-negative control",
                ));
            }
            build_edge_evidence_record(
                candidate.left_surface_id.clone(),
                candidate.right_surface_id.clone(),
                hits,
            )
        })
        .collect()
}

fn native_solve_provenance(surfaces: &[NativeScaleSurface]) -> Vec<SolveSurfaceProvenance> {
    surfaces
        .iter()
        .map(|surface| SolveSurfaceProvenance {
            surface_id: surface.surface_id.clone(),
            row_count: surface.row_count,
            deal_count: native_usize_to_u64(surface.source_ordinals.len()),
        })
        .collect()
}

fn native_scale_stage_metrics(proof: &NativeScaleProof) -> Vec<NativeScaleStageMetric> {
    vec![
        NativeScaleStageMetric {
            stage: "intake".to_string(),
            input_records: proof.intake.observation_count,
            output_records: proof.intake.unique_surface_count,
            artifact_bytes: serialized_len_lossy(&proof.intake),
            disk_write_bytes: 0,
            wall_time_ms: None,
            peak_rss_bytes: None,
        },
        NativeScaleStageMetric {
            stage: "index".to_string(),
            input_records: proof.intake.unique_surface_count,
            output_records: proof
                .index
                .token_count
                .saturating_add(proof.index.ngram_count),
            artifact_bytes: proof.index.artifact_bytes,
            disk_write_bytes: proof.index.artifact_bytes,
            wall_time_ms: None,
            peak_rss_bytes: None,
        },
        NativeScaleStageMetric {
            stage: "block".to_string(),
            input_records: proof.intake.unique_surface_count,
            output_records: proof.block.candidate_record_count,
            artifact_bytes: proof.block.candidate_artifact_bytes,
            disk_write_bytes: proof.block.candidate_artifact_bytes,
            wall_time_ms: None,
            peak_rss_bytes: None,
        },
        NativeScaleStageMetric {
            stage: "evidence".to_string(),
            input_records: proof.block.candidate_record_count,
            output_records: proof.edge_record_count,
            artifact_bytes: serialized_len_lossy(&proof.solve.diagnostics.summary),
            disk_write_bytes: serialized_len_lossy(&proof.solve.diagnostics.summary),
            wall_time_ms: None,
            peak_rss_bytes: None,
        },
        NativeScaleStageMetric {
            stage: "solve".to_string(),
            input_records: proof.edge_record_count,
            output_records: proof.solve.solved_component_count,
            artifact_bytes: serialized_len_lossy(&proof.solve),
            disk_write_bytes: serialized_len_lossy(&proof.solve),
            wall_time_ms: None,
            peak_rss_bytes: None,
        },
        NativeScaleStageMetric {
            stage: "review".to_string(),
            input_records: proof.solve.solved_component_count,
            output_records: proof.solve.review_group_count,
            artifact_bytes: serialized_len_lossy(&proof.solve.diagnostics.review_group_seeds),
            disk_write_bytes: serialized_len_lossy(&proof.solve.diagnostics.review_group_seeds),
            wall_time_ms: None,
            peak_rss_bytes: None,
        },
        NativeScaleStageMetric {
            stage: "artifact_publication".to_string(),
            input_records: 6,
            output_records: 1,
            artifact_bytes: proof.artifact_publication.artifact_bytes,
            disk_write_bytes: proof.artifact_publication.disk_write_bytes,
            wall_time_ms: None,
            peak_rss_bytes: None,
        },
    ]
}

fn finalize_native_scale_proof(proof: &mut NativeScaleProof) -> Result<(), Refusal> {
    for _ in 0..4 {
        proof.artifact_content_hash = hash_native_scale_proof_without_self(proof)?;
        proof.artifact_publication.deterministic_content_hash = proof.artifact_content_hash.clone();
        let artifact_bytes = serialized_len(proof)?;
        proof.artifact_publication.artifact_bytes = artifact_bytes;
        proof.artifact_publication.disk_write_bytes = artifact_bytes;
        proof.stage_metrics = native_scale_stage_metrics(proof);
    }
    proof.artifact_content_hash = hash_native_scale_proof_without_self(proof)?;
    proof.artifact_publication.deterministic_content_hash = proof.artifact_content_hash.clone();
    Ok(())
}

fn hash_native_scale_proof_without_self(proof: &NativeScaleProof) -> Result<String, Refusal> {
    let mut hashable = proof.clone();
    hashable.artifact_content_hash.clear();
    hashable
        .artifact_publication
        .deterministic_content_hash
        .clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        native_scale_artifact_refusal(
            "Failed to hash native scale proof artifact",
            json!({
                "stage": "artifact_publication",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn native_cache_key_material(config: &NativeScaleProofConfig, generator_hash: &str) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}",
        CANON_ENTITY_NATIVE_SCALE_PROOF_VERSION,
        generator_hash,
        config.source_count,
        config.entity_count,
        config.variants_per_entity,
        config.ngram_width
    )
}

fn native_input_hash(
    config: &NativeScaleProofConfig,
    report: &NativeScaleIntakeReport,
    surfaces: &[NativeScaleSurface],
) -> String {
    let material = format!(
        "{}:{}:{}:{}:{}:{}:{}:{}",
        CANON_ENTITY_NATIVE_SCALE_GENERATOR_VERSION,
        config.observation_count,
        config.source_count,
        config.entity_count,
        config.variants_per_entity,
        report.unique_surface_count,
        report.multisource_entity_count,
        surfaces
            .last()
            .map(|surface| surface.surface_id.as_str())
            .unwrap_or("")
    );
    witness::hash_bytes(material.as_bytes())
}

fn native_scale_reproducible_commands() -> Vec<String> {
    vec![
        "cargo test --test entity_scale_e2e".to_string(),
        "cargo clippy --test entity_scale_e2e -- -D warnings".to_string(),
        "cargo test --bench entity_native --no-run".to_string(),
    ]
}

fn serialized_len<T: Serialize>(value: &T) -> Result<u64, Refusal> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .map_err(|error| {
            native_scale_artifact_refusal(
                "Failed to measure native scale artifact bytes",
                json!({
                    "stage": "artifact_publication",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })
}

fn serialized_len_lossy<T: Serialize>(value: &T) -> u64 {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .unwrap_or_default()
}

fn native_scale_artifact_refusal(message: impl Into<String>, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("Rerun the native scale proof with deterministic offline inputs".to_string()),
    )
}

fn native_usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn block_stage_request_to_run_request<'a>(
    request: EntityBlockStageRequest<'a>,
) -> EntityRunRequest<'a> {
    EntityRunRequest {
        rows: request.rows,
        profile: request.profile,
        strategy: request.strategy,
        registry: request.registry,
        work_dir: request.work_dir,
    }
}

fn evidence_stage_request_to_run_request<'a>(
    request: EntityEvidenceStageRequest<'a>,
) -> EntityRunRequest<'a> {
    EntityRunRequest {
        rows: request.rows,
        profile: request.profile,
        strategy: request.strategy,
        registry: request.registry,
        work_dir: request.work_dir,
    }
}

fn solve_stage_request_to_run_request<'a>(
    request: EntitySolveStageRequest<'a>,
) -> EntityRunRequest<'a> {
    EntityRunRequest {
        rows: request.rows,
        profile: request.profile,
        strategy: request.strategy,
        registry: request.registry,
        work_dir: request.work_dir,
    }
}

fn read_block_stage_from_artifact(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    index: &EntityIndexRun,
    surfaces: &[PreparedSurfaceRecord],
    block_artifact_path: &Path,
) -> Result<EntityBlockRun, Refusal> {
    let artifact_value: Value = read_json_file(block_artifact_path, "block artifact")?;
    let contract = validate_artifact_v1_core_contract(&artifact_value)?;
    if contract.stage != EntityArtifactStageV1::Block {
        return Err(stage_context_refusal(
            "block",
            "version",
            json!(CANON_ENTITY_BLOCK_VERSION_V1),
            artifact_value
                .get("version")
                .cloned()
                .unwrap_or(Value::Null),
        ));
    }
    validate_entity_v1_self_hash(&artifact_value)?;
    let artifact: BlockCandidateArtifact =
        deserialize_artifact_value(&artifact_value, "block", "block artifact")?;
    validate_block_candidate_artifact_envelope_contract(&artifact)?;
    validate_stage_metadata_context(
        "block",
        &artifact.metadata,
        &index.artifact.metadata,
        &stage_strategy(base_strategy, "block"),
    )?;
    validate_upstream_artifacts(
        "block",
        &artifact.upstream_artifacts,
        expected_block_upstreams(&index.artifact),
    )?;

    let candidate_records_path = resolve_work_dir_artifact_path(
        request.work_dir,
        &artifact.candidate_records_path,
        "candidate_records_path",
        "block",
    )?;
    let candidate_diagnostics_path = resolve_work_dir_artifact_path(
        request.work_dir,
        &artifact.candidate_diagnostics_path,
        "candidate_diagnostics_path",
        "block",
    )?;
    let exact_buckets_path = resolve_work_dir_artifact_path(
        request.work_dir,
        BLOCK_EXACT_BUCKETS_PATH,
        "exact_bucket_assertions_path",
        "block",
    )?;
    let candidates: Vec<crate::entity::block::BlockCandidateRecord> = read_logical_jsonl_file(
        request,
        &artifact.candidate_records_path,
        &candidate_records_path,
        "block candidate records",
    )?;
    let diagnostics: crate::entity::block::BlockCandidateGenerationDiagnostics =
        read_logical_json_file(
            request,
            &artifact.candidate_diagnostics_path,
            &candidate_diagnostics_path,
            "block candidate diagnostics",
        )?;
    let exact_buckets: Vec<ExactBucketAssertion> = read_logical_jsonl_file(
        request,
        BLOCK_EXACT_BUCKETS_PATH,
        &exact_buckets_path,
        "exact bucket assertions",
    )?;
    validate_block_candidate_payload_hashes(&artifact, &candidates, &diagnostics, &exact_buckets)?;
    validate_block_payload_surfaces(&candidates, &exact_buckets, surfaces)?;

    Ok(EntityBlockRun {
        artifact,
        artifact_value,
        candidates,
        exact_buckets,
        record_link_candidate_set: read_record_link_candidate_set(request, base_strategy)?,
        publication_context: read_cache_execution_receipt_context(request)?,
        publication_files: Vec::new(),
    })
}

fn read_edge_stage_from_artifact(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    prepare: &PrepareRunArtifact,
    edge_artifact_path: &Path,
) -> Result<
    (
        EdgeEvidenceArtifact,
        Value,
        Vec<EdgeEvidenceRecord>,
        Vec<ExactBucketAssertion>,
    ),
    Refusal,
> {
    let artifact_value: Value = read_json_file(edge_artifact_path, "evidence artifact")?;
    let contract = validate_artifact_v1_core_contract(&artifact_value)?;
    if contract.stage != EntityArtifactStageV1::Evidence {
        return Err(stage_context_refusal(
            "evidence",
            "version",
            json!(CANON_ENTITY_EVIDENCE_VERSION_V1),
            artifact_value
                .get("version")
                .cloned()
                .unwrap_or(Value::Null),
        ));
    }
    validate_entity_v1_self_hash(&artifact_value)?;
    let artifact: EdgeEvidenceArtifact =
        deserialize_artifact_value(&artifact_value, "evidence", "evidence artifact")?;
    validate_edge_evidence_artifact_envelope_contract(&artifact)?;
    validate_stage_metadata_context(
        "evidence",
        &artifact.metadata,
        &prepare.metadata,
        &stage_strategy(base_strategy, "evidence"),
    )?;

    let edge_records_path = resolve_work_dir_artifact_path(
        request.work_dir,
        &artifact.edge_records_path,
        "evidence_records_path",
        "evidence",
    )?;
    let exact_buckets_path = resolve_work_dir_artifact_path(
        request.work_dir,
        BLOCK_EXACT_BUCKETS_PATH,
        "exact_bucket_assertions_path",
        "evidence",
    )?;
    let edge_records: Vec<EdgeEvidenceRecord> = read_logical_jsonl_file(
        request,
        &artifact.edge_records_path,
        &edge_records_path,
        "evidence records",
    )?;
    let exact_buckets: Vec<ExactBucketAssertion> = read_logical_jsonl_file(
        request,
        BLOCK_EXACT_BUCKETS_PATH,
        &exact_buckets_path,
        "exact bucket assertions",
    )?;
    validate_edge_evidence_payload_hashes(&artifact, &edge_records, &exact_buckets)?;

    Ok((artifact, artifact_value, edge_records, exact_buckets))
}

fn validate_stage_metadata_context(
    stage: &'static str,
    actual: &EntityArtifactMetadata,
    expected: &EntityArtifactMetadata,
    expected_strategy: &EntityStrategyReference,
) -> Result<(), Refusal> {
    if actual.profile != expected.profile {
        return Err(stage_context_refusal(
            stage,
            "metadata.profile",
            json!(&expected.profile),
            json!(&actual.profile),
        ));
    }
    if actual.strategy != *expected_strategy {
        return Err(stage_context_refusal(
            stage,
            "metadata.strategy",
            json!(expected_strategy),
            json!(&actual.strategy),
        ));
    }
    if actual.registry_snapshot != expected.registry_snapshot {
        return Err(stage_context_refusal(
            stage,
            "metadata.registry_snapshot",
            json!(&expected.registry_snapshot),
            json!(&actual.registry_snapshot),
        ));
    }
    if actual.patch_namespace != expected.patch_namespace {
        return Err(stage_context_refusal(
            stage,
            "metadata.patch_namespace",
            json!(&expected.patch_namespace),
            json!(&actual.patch_namespace),
        ));
    }
    if actual.input != expected.input {
        return Err(stage_context_refusal(
            stage,
            "metadata.input",
            json!(&expected.input),
            json!(&actual.input),
        ));
    }
    if actual.patch_set != expected.patch_set {
        return Err(stage_context_refusal(
            stage,
            "metadata.patch_set",
            json!(&expected.patch_set),
            json!(&actual.patch_set),
        ));
    }
    if actual.namekit != expected.namekit {
        return Err(stage_context_refusal(
            stage,
            "metadata.namekit",
            json!(&expected.namekit),
            json!(&actual.namekit),
        ));
    }
    Ok(())
}

fn validate_upstream_artifacts(
    stage: &'static str,
    actual: &[EntityArtifactReference],
    expected: Vec<EntityArtifactReference>,
) -> Result<(), Refusal> {
    let mut actual = actual.to_vec();
    let mut expected = expected;
    actual.sort_by(artifact_ref_cmp);
    expected.sort_by(artifact_ref_cmp);
    if actual != expected {
        return Err(stage_context_refusal(
            stage,
            "metadata.upstream_artifacts",
            json!(expected),
            json!(actual),
        ));
    }
    Ok(())
}

fn expected_block_upstreams(index: &EntityIndexArtifact) -> Vec<EntityArtifactReference> {
    let mut upstreams = index.metadata.upstream_artifacts.clone();
    upstreams.push(EntityArtifactReference {
        version: index.version.clone(),
        content_hash: index.artifact_content_hash.clone(),
    });
    upstreams
}

fn validate_block_payload_surfaces(
    candidates: &[crate::entity::block::BlockCandidateRecord],
    exact_buckets: &[ExactBucketAssertion],
    surfaces: &[PreparedSurfaceRecord],
) -> Result<(), Refusal> {
    let surface_ids = surfaces
        .iter()
        .map(|surface| surface.surface_id.as_str())
        .collect::<BTreeSet<_>>();
    for candidate in candidates {
        for surface_id in [&candidate.left_surface_id, &candidate.right_surface_id] {
            if !surface_ids.contains(surface_id.as_str()) {
                return Err(stage_context_refusal(
                    "block",
                    "candidate_records.surface_id",
                    json!("known prepared surface"),
                    json!(surface_id),
                ));
            }
        }
    }
    for bucket in exact_buckets {
        for surface_id in &bucket.membership.surface_ids {
            if !surface_ids.contains(surface_id.as_str()) {
                return Err(stage_context_refusal(
                    "block",
                    "exact_bucket_assertions.surface_id",
                    json!("known prepared surface"),
                    json!(surface_id),
                ));
            }
        }
        for range in &bucket.membership.surface_ranges {
            for surface_id in [&range.start_surface_id, &range.end_surface_id] {
                if !surface_ids.contains(surface_id.as_str()) {
                    return Err(stage_context_refusal(
                        "block",
                        "exact_bucket_assertions.surface_range",
                        json!("known prepared surface"),
                        json!(surface_id),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn resolve_work_dir_artifact_path(
    work_dir: &Path,
    relative: &str,
    field: &'static str,
    stage: &'static str,
) -> Result<std::path::PathBuf, Refusal> {
    let path = Path::new(relative);
    if relative.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity stage artifact path must be a safe relative path",
            json!({
                "stage": stage,
                "field": field,
                "path": relative,
                "writes_performed": false
            }),
            Some("Use work-dir relative entity artifact paths and rerun the stage".to_string()),
        ));
    }
    Ok(work_dir.join(path))
}

fn stage_context_refusal(
    stage: &'static str,
    field: &'static str,
    expected: serde_json::Value,
    actual: serde_json::Value,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Entity stage artifact does not match current stage inputs",
        json!({
            "stage": stage,
            "field": field,
            "expected": expected,
            "actual": actual,
            "writes_performed": false
        }),
        Some(format!(
            "Rerun canon entity {stage} with matching upstream artifacts"
        )),
    )
}

fn deserialize_artifact_value<T: DeserializeOwned>(
    value: &Value,
    stage: &'static str,
    label: &'static str,
) -> Result<T, Refusal> {
    serde_json::from_value(value.clone()).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            format!("Failed to deserialize {label}"),
            json!({
                "stage": stage,
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(next_stage_command(stage)),
        )
    })
}

fn artifact_reference_chain(source: &Value) -> Result<Vec<EntityArtifactReferenceV1>, Refusal> {
    let metadata = source
        .get("metadata")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Entity v1 source artifact is missing metadata",
                json!({
                    "stage": "artifact_chain",
                    "field": "metadata",
                    "writes_performed": false
                }),
                Some("Rerun the previous canon entity stage".to_string()),
            )
        })?;
    let mut upstreams = metadata
        .get("upstream_artifacts")
        .map(|value| serde_json::from_value::<Vec<EntityArtifactReferenceV1>>(value.clone()))
        .transpose()
        .map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Entity v1 source upstream references failed to deserialize",
                json!({
                    "stage": "artifact_chain",
                    "field": "metadata.upstream_artifacts",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some("Rerun the previous canon entity stage".to_string()),
            )
        })?
        .unwrap_or_default();
    upstreams.push(entity_v1_artifact_reference(source)?);
    sort_entity_v1_upstream_references(upstreams)
}

fn publish_v1_stage_artifact<T>(
    artifact: T,
    stage: EntityArtifactStageV1,
    source: &Value,
    strategy: EntityStrategyReference,
    upstreams: Vec<EntityArtifactReferenceV1>,
) -> Result<(T, Value), Refusal>
where
    T: Serialize + DeserializeOwned,
{
    let contract = entity_v1_contract_for_stage(stage)?;
    let mut value = serde_json::to_value(&artifact).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to serialize entity v1 stage artifact",
            json!({
                "stage": stage.as_str(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(next_stage_command(stage.as_str())),
        )
    })?;
    let mut metadata = entity_v1_lifecycle_metadata_from_source(source, stage, upstreams)?;
    if let Some(metadata_object) = metadata.as_object_mut() {
        metadata_object.insert(
            "strategy".to_string(),
            serde_json::to_value(strategy).expect("strategy serializes"),
        );
    }
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "version".to_string(),
            Value::String(contract.artifact_version.to_string()),
        );
        object.insert(
            "artifact_content_hash".to_string(),
            Value::String(String::new()),
        );
        object.insert("metadata".to_string(), metadata.clone());
        if object.contains_key("upstream_artifacts") {
            object.insert(
                "upstream_artifacts".to_string(),
                metadata
                    .get("upstream_artifacts")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new())),
            );
        }
    }
    finalize_entity_v1_self_hash(&mut value)?;
    validate_artifact_v1_core_contract(&value)?;
    validate_entity_v1_self_hash(&value)?;
    let typed = deserialize_artifact_value(&value, stage.as_str(), "entity v1 stage artifact")?;
    Ok((typed, value))
}

fn next_stage_command(stage: &str) -> String {
    format!("Rerun canon entity {stage} with matching v1 artifacts")
}

fn build_and_write_index(
    request: EntityRunRequest<'_>,
    _base_strategy: &BaseStrategyReference,
    _prepare: &EntityArtifactHeader,
    surfaces: &[PreparedSurfaceRecord],
    cache_mode: EntityIndexCacheMode,
) -> Result<EntityIndexRun, Refusal> {
    let result = run_index_build_v1_with_cache_mode(
        EntityIndexBuildRequest {
            rows: request.rows,
            profile: request.profile,
            strategy: request.strategy,
            registry: request.registry,
            work_dir: request.work_dir,
            max_artifact_bytes: None,
        },
        cache_mode,
    )?;
    let artifact = deserialize_artifact_value::<EntityIndexArtifact>(
        &result.artifact,
        "index",
        "entity index v1 artifact",
    )?;
    let posting_surfaces = posting_surfaces(surfaces);
    let ngram_surfaces = ngram_surfaces(surfaces);
    let postings = EntityPostingIndex::build(
        &posting_surfaces,
        EntityPostingBuildConfig {
            common_posting_limit: 100,
        },
    )
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to build entity postings index",
            json!({ "stage": "index", "error": format!("{error:?}"), "writes_performed": false }),
            Some(next_run_command(request)),
        )
    })?;
    let ngrams = EntityNgramIndex::build(
        &ngram_surfaces,
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(3).expect("3-gram config is valid"),
            common_posting_limit: 100,
        },
    )
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to build entity ngram index",
            json!({ "stage": "index", "error": format!("{error:?}"), "writes_performed": false }),
            Some(next_run_command(request)),
        )
    })?;
    let cache_bundle_receipt_content_hash = witness::hash_file(&result.paths.receipt_path)
        .map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Failed to hash entity index v1 immutable cache bundle receipt",
                json!({
                    "stage": "index",
                    "path": result.paths.receipt_path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some(next_run_command(request)),
            )
        })?;
    let cache_status = match cache_mode {
        EntityIndexCacheMode::Enabled => result.cache_status,
        EntityIndexCacheMode::Disabled => EntityIndexCacheStatus::Bypassed,
    };
    let cache_execution_receipt_content_hash = write_cache_execution_receipt(
        request,
        &result.paths.receipt_path,
        cache_mode,
        cache_status,
    )?;

    Ok(EntityIndexRun {
        artifact,
        artifact_value: result.artifact,
        postings,
        ngrams,
        cache_mode,
        cache_status,
        cache_execution_receipt_path: RUN_CACHE_EXECUTION_RECEIPT_PATH.to_string(),
        cache_execution_receipt_content_hash,
        cache_bundle_receipt_path: INDEX_CACHE_RECEIPT_FILE.to_string(),
        cache_bundle_receipt_content_hash,
    })
}

fn write_cache_execution_receipt(
    request: EntityRunRequest<'_>,
    bundle_receipt_path: &Path,
    cache_mode: EntityIndexCacheMode,
    cache_status: EntityIndexCacheStatus,
) -> Result<String, Refusal> {
    let mut receipt: EntityIndexCacheReceipt =
        read_json_file(bundle_receipt_path, "entity index v1 cache bundle receipt")?;
    receipt.mode = cache_mode;
    receipt.status = cache_status;
    receipt.reusable = cache_mode == EntityIndexCacheMode::Enabled;
    let execution_path = request.work_dir.join(RUN_CACHE_EXECUTION_RECEIPT_PATH);
    write_json_file(&execution_path, &receipt)?;
    witness::hash_file(&execution_path).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash entity run cache execution receipt",
            json!({
                "stage": "index",
                "path": execution_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(next_run_command(request)),
        )
    })
}

fn index_publication_context(index: &EntityIndexRun) -> EntityRunPublicationContext {
    EntityRunPublicationContext {
        cache_mode: index.cache_mode,
        cache_status: index.cache_status,
        cache_receipt_hash: index.cache_execution_receipt_content_hash.clone(),
    }
}

fn read_cache_execution_receipt_context(
    request: EntityRunRequest<'_>,
) -> Result<EntityRunPublicationContext, Refusal> {
    let execution_path = request.work_dir.join(RUN_CACHE_EXECUTION_RECEIPT_PATH);
    let receipt: EntityIndexCacheReceipt =
        read_json_file(&execution_path, "entity run cache execution receipt")?;
    let cache_receipt_hash = witness::hash_file(&execution_path).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash entity run cache execution receipt",
            json!({
                "stage": "index",
                "path": execution_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(next_run_command(request)),
        )
    })?;
    Ok(EntityRunPublicationContext {
        cache_mode: receipt.mode,
        cache_status: receipt.status,
        cache_receipt_hash,
    })
}

fn publication_upstream_ref(
    version: impl Into<String>,
    content_hash: impl Into<String>,
) -> EntityPublicationUpstreamRef {
    EntityPublicationUpstreamRef {
        version: version.into(),
        content_hash: content_hash.into(),
    }
}

fn build_and_write_block(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    index: &EntityIndexRun,
    surfaces: &[PreparedSurfaceRecord],
    mirror_stable_paths: bool,
) -> Result<EntityBlockRun, Refusal> {
    let strategy = stage_strategy(base_strategy, "block");
    let block_config = load_block_runtime_config(request.strategy)?;
    let mut result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: index.artifact.metadata.profile.id.clone(),
        posting_index: &index.postings,
        ngram_index: Some(&index.ngrams),
        budget_config: block_config.candidate_budget,
        operators: default_block_candidate_operators(core_view_name(
            &index.artifact.metadata.profile.id,
        )),
    })?;
    for candidate in &mut result.candidates {
        candidate.version = CANON_ENTITY_BLOCK_VERSION_V1.to_string();
    }
    let record_link = build_record_link_block_run(request, base_strategy, index, surfaces)?;
    if let Some(record_link) = &record_link {
        merge_block_candidates(
            &mut result.candidates,
            record_link_block_candidates(record_link)?,
        );
        let record_link_suppressed = record_link
            .candidate_set
            .pair_accounting
            .suppressed_pair_count
            .saturating_add(record_link.candidate_set.abstentions.len() as u64);
        result.diagnostics.candidate_record_count = result.candidates.len() as u64;
        result.diagnostics.candidate_pairs_emitted = result.candidates.len() as u64;
        result.diagnostics.suppressed_candidate_count = result
            .diagnostics
            .suppressed_candidate_count
            .saturating_add(record_link_suppressed);
        result.diagnostics.max_candidates_for_operator = result
            .diagnostics
            .max_candidates_for_operator
            .max(record_link.candidate_set.candidates.len() as u64);
        let operator_id = record_link_block_operator_id(record_link);
        result
            .diagnostics
            .operator_yield
            .push(crate::entity::block::BlockOperatorYield {
                operator_id: operator_id.clone(),
                emitted_candidate_count: record_link.candidate_set.candidates.len() as u64,
                suppressed_candidate_count: record_link_suppressed,
                large_posting_suppressed_count: 0,
            });
        result.diagnostics.operator_diagnostics.push(
            crate::entity::block::BlockOperatorCandidateDiagnostics {
                operator_id,
                input_candidate_count: record_link
                    .candidate_set
                    .pair_accounting
                    .cross_source_pair_count,
                eligible_candidate_count: record_link
                    .candidate_set
                    .pair_accounting
                    .admitted_pair_count,
                emitted_candidate_count: record_link.candidate_set.candidates.len() as u64,
                suppressed_candidate_count: record_link_suppressed,
                large_posting_suppressed_count: 0,
            },
        );
        result
            .diagnostics
            .operator_diagnostics
            .sort_by(|left, right| left.operator_id.cmp(&right.operator_id));
        result
            .diagnostics
            .operator_yield
            .sort_by(|left, right| left.operator_id.cmp(&right.operator_id));
    }
    let exact_bucket_result = emit_exact_bucket_hyperedges(ExactBucketBlockRequest {
        profile: exact_bucket_profile(&index.artifact.metadata),
        upstream: ExactBucketUpstream {
            prepare_hash: index
                .artifact
                .metadata
                .upstream_artifacts
                .iter()
                .find(|reference| reference.version == CANON_ENTITY_PREPARE_VERSION_V1)
                .map(|reference| reference.content_hash.clone())
                .unwrap_or_default(),
            index_hash: index.artifact.artifact_content_hash.clone(),
            strategy_hash: strategy.content_hash.clone(),
            registry_snapshot_hash: index
                .artifact
                .metadata
                .registry_snapshot
                .lookup_snapshot_hash
                .clone(),
        },
        operator_id: format!(
            "exact_view:{}",
            core_view_name(&index.artifact.metadata.profile.id)
        ),
        identity_view: core_view_name(&index.artifact.metadata.profile.id).to_string(),
        placeholder_values: placeholder_bucket_values(),
        surfaces: exact_bucket_surfaces(&index.artifact.metadata.profile.id, surfaces),
    })
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to emit exact bucket assertions",
            json!({ "stage": "block", "error": format!("{error:?}"), "writes_performed": false }),
            Some(next_run_command(request)),
        )
    })?;
    let artifact = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: EntityArtifactHeader {
            version: index.artifact.version.clone(),
            metadata: index.artifact.metadata.clone(),
            summary: index.artifact.summary.clone(),
        },
        strategy,
        candidate_records_path: BLOCK_CANDIDATES_PATH.to_string(),
        candidate_diagnostics_path: BLOCK_DIAGNOSTICS_PATH.to_string(),
        candidate_records: result.candidates.clone(),
        bucket_assertions: exact_bucket_result.assertions.clone(),
        known_surface_ids: surfaces
            .iter()
            .map(|surface| surface.surface_id.clone())
            .collect(),
        diagnostics: result.diagnostics.clone(),
    })?;
    validate_block_candidate_artifact_contract(&artifact)?;
    let (artifact, artifact_value) = publish_v1_stage_artifact(
        artifact,
        EntityArtifactStageV1::Block,
        &index.artifact_value,
        stage_strategy(base_strategy, "block"),
        artifact_reference_chain(&index.artifact_value)?,
    )?;
    let mut publication_files = vec![jsonl_publication_file(
        BLOCK_CANDIDATES_PATH,
        "block",
        CANON_ENTITY_BLOCK_VERSION_V1,
        &result.candidates,
    )?];
    if let Some(record_link) = &record_link {
        let bytes = canonical_record_link_candidate_set_bytes(&record_link.candidate_set)
            .map_err(|error| record_link_refusal(error, "block", request))?;
        publication_files.push(EntityPublicationFileInput::new(
            RECORD_LINK_CANDIDATES_PATH,
            "block",
            RECORD_LINK_CANDIDATE_SET_VERSION,
            bytes,
        ));
    }
    publication_files.push(json_publication_file(
        BLOCK_DIAGNOSTICS_PATH,
        "block",
        CANON_ENTITY_BLOCK_VERSION_V1,
        &result.diagnostics,
    )?);
    publication_files.push(jsonl_publication_file(
        BLOCK_EXACT_BUCKETS_PATH,
        "block",
        CANON_ENTITY_BLOCK_VERSION_V1,
        &exact_bucket_result.assertions,
    )?);
    publication_files.push(json_publication_file(
        BLOCK_ARTIFACT_PATH,
        "block",
        CANON_ENTITY_BLOCK_VERSION_V1,
        &artifact_value,
    )?);
    let publication_context = index_publication_context(index);
    if mirror_stable_paths {
        let publication = publish_manual_stage_generation(
            request,
            &publication_context,
            vec![
                publication_upstream_ref(
                    CANON_ENTITY_INDEX_VERSION_V1,
                    index.artifact.artifact_content_hash.clone(),
                ),
                publication_upstream_ref(
                    CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION,
                    index.cache_execution_receipt_content_hash.clone(),
                ),
            ],
            &publication_files,
        )?;
        mirror_publication_files(request, &publication, &publication_files)?;
    }

    Ok(EntityBlockRun {
        artifact,
        artifact_value,
        candidates: result.candidates,
        exact_buckets: exact_bucket_result.assertions,
        record_link_candidate_set: record_link.map(|record_link| record_link.candidate_set),
        publication_context,
        publication_files,
    })
}

fn build_record_link_block_run(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    index: &EntityIndexRun,
    surfaces: &[PreparedSurfaceRecord],
) -> Result<Option<RecordLinkBlockRun>, Refusal> {
    let Some(config) = &base_strategy.record_link else {
        return Ok(None);
    };
    let input_set = load_record_link_inputs(RecordLinkLoadRequest {
        workspace_root: strategy_workspace_root(request.strategy),
        sidecar_paths: config.input_paths.clone(),
        expected_profile_id: Some(index.artifact.metadata.profile.id.clone()),
        expected_profile_digest: index.artifact.metadata.profile.content_hash.clone(),
        expected_scope_id: None,
    })
    .map_err(|error| record_link_refusal(error, "block", request))?;
    let loaded_profile = load_prepare_profile_with_hash(request.profile)?;
    if index.artifact.metadata.profile.content_hash != Some(loaded_profile.content_hash.clone()) {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Record-link binding replay profile no longer matches the indexed prepare profile",
            json!({
                "stage": "block",
                "field": "profile.content_hash",
                "expected": index.artifact.metadata.profile.content_hash.clone(),
                "actual": loaded_profile.content_hash.clone(),
                "writes_performed": false
            }),
            Some(next_run_command(request)),
        ));
    }
    let prepare_contract = prepare_contract_for_loaded_profile(&loaded_profile)?;
    let observations = project_prepare_path(request.rows, &prepare_contract)?;
    let surface_index = bind_record_link_surfaces(
        &input_set,
        &record_link_surface_bindings(&observations, surfaces),
        "block",
    )
    .map_err(|error| record_link_refusal(error, "block", request))?;
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: config.candidate_config.clone(),
    })
    .map_err(|error| record_link_refusal(error, "block", request))?;
    Ok(Some(RecordLinkBlockRun {
        input_set,
        candidate_set,
    }))
}

fn record_link_block_candidates(
    record_link: &RecordLinkBlockRun,
) -> Result<Vec<BlockCandidateRecord>, Refusal> {
    record_link
        .candidate_set
        .candidates
        .iter()
        .filter(|candidate| candidate.left.surface_id != candidate.right.surface_id)
        .map(|candidate| {
            let (left_surface_id, right_surface_id) =
                ordered_surface_pair(&candidate.left.surface_id, &candidate.right.surface_id);
            let score_units = u32::try_from(candidate.score_hint_units).unwrap_or(u32::MAX);
            Ok(BlockCandidateRecord {
                version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
                left_surface_id,
                right_surface_id,
                block_hits: vec![BlockCandidateHit {
                    operator_id: format!(
                        "record_link:{}:{}",
                        record_link.candidate_set.content_hash, candidate.candidate_id
                    ),
                    rank: Some(1),
                    score_units,
                }],
                candidate_score_hint: score_units,
            })
        })
        .collect()
}

fn merge_block_candidates(
    candidates: &mut Vec<BlockCandidateRecord>,
    record_link_candidates: Vec<BlockCandidateRecord>,
) {
    let mut by_pair = BTreeMap::<(String, String), BlockCandidateRecord>::new();
    for candidate in candidates.drain(..).chain(record_link_candidates) {
        let key = (
            candidate.left_surface_id.clone(),
            candidate.right_surface_id.clone(),
        );
        by_pair
            .entry(key)
            .and_modify(|existing| {
                existing.block_hits.extend(candidate.block_hits.clone());
                existing.block_hits.sort_by(|left, right| {
                    left.operator_id
                        .cmp(&right.operator_id)
                        .then_with(|| left.rank.cmp(&right.rank))
                });
                existing
                    .block_hits
                    .dedup_by(|left, right| left.operator_id == right.operator_id);
                existing.candidate_score_hint = existing
                    .block_hits
                    .iter()
                    .map(|hit| hit.score_units)
                    .max()
                    .unwrap_or_default();
            })
            .or_insert(candidate);
    }
    candidates.extend(by_pair.into_values());
    candidates.sort_by(block_candidate_record_cmp);
}

fn record_link_block_operator_id(record_link: &RecordLinkBlockRun) -> String {
    format!("record_link:{}", record_link.candidate_set.content_hash)
}

fn record_link_surface_bindings(
    observations: &[PreparedInputObservation],
    surfaces: &[PreparedSurfaceRecord],
) -> Vec<RecordLinkSurfaceBindingInput> {
    let mut surface_ids_by_primary_surface = BTreeMap::<(String, String), BTreeSet<String>>::new();
    for surface in surfaces {
        for raw_variant in &surface.raw_variants {
            if !raw_variant.trim().is_empty() {
                surface_ids_by_primary_surface
                    .entry((surface.profile_id.clone(), raw_variant.clone()))
                    .or_default()
                    .insert(surface.surface_id.clone());
            }
        }
    }

    let mut record_keys_by_surface_source = BTreeMap::<(String, String), BTreeSet<String>>::new();
    for observation in observations {
        let Some(source_row_id) = observation
            .provenance
            .get("source_row_id")
            .filter(|source_row_id| !source_row_id.trim().is_empty())
        else {
            continue;
        };
        let source_id = observation
            .provenance
            .get("source_system")
            .filter(|source_id| !source_id.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "source".to_string());
        let key = (
            observation.profile_id.clone(),
            observation.primary_surface.value.clone(),
        );
        let Some(surface_ids) = surface_ids_by_primary_surface.get(&key) else {
            continue;
        };
        for surface_id in surface_ids {
            record_keys_by_surface_source
                .entry((surface_id.clone(), source_id.clone()))
                .or_default()
                .insert(source_row_id.clone());
        }
    }

    record_keys_by_surface_source
        .into_iter()
        .map(
            |((surface_id, source_id), source_row_ids)| RecordLinkSurfaceBindingInput {
                source_id,
                surface_id,
                source_row_ids: source_row_ids.into_iter().collect(),
            },
        )
        .collect()
}

fn strategy_workspace_root(strategy: &Path) -> &Path {
    strategy.parent().unwrap_or_else(|| Path::new("."))
}

fn ordered_surface_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn record_link_refusal(
    error: RecordLinkCoreError,
    stage: &'static str,
    request: EntityRunRequest<'_>,
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
        Some(next_run_command(request)),
    )
}

fn build_and_write_edge(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    block: &EntityBlockRun,
    surfaces: &[PreparedSurfaceRecord],
    mirror_stable_paths: bool,
) -> Result<
    (
        EdgeEvidenceArtifact,
        Value,
        Vec<EdgeEvidenceRecord>,
        Vec<EntityPublicationFileInput>,
    ),
    Refusal,
> {
    let strategy = stage_strategy(base_strategy, "evidence");
    let loaded_profile = load_prepare_profile_with_hash(request.profile)?;
    validate_edge_profile_binding(&loaded_profile, &block.artifact.metadata.profile)?;
    let relation_namespace = block
        .artifact
        .metadata
        .profile
        .patch_namespaces
        .relations
        .clone();
    let support_namespace = block
        .artifact
        .metadata
        .profile
        .patch_namespaces
        .aliases
        .clone();
    let scoring_context = EdgeSupportScoringContext::new(
        &loaded_profile.document,
        &support_namespace,
        &relation_namespace,
        surfaces,
    )?;
    let mut edge_records = block
        .candidates
        .iter()
        .map(|candidate| edge_record_for_candidate(candidate, &scoring_context))
        .collect::<Result<Vec<_>, _>>()?;
    let record_link_evidence = build_record_link_evidence_run(request, base_strategy, block)?;
    if let Some(record_link_evidence) = &record_link_evidence {
        merge_record_link_edge_hits(
            &mut edge_records,
            &record_link_evidence.edge_hits,
            base_strategy
                .record_link
                .as_ref()
                .map(|config| config.assignment_hint_score_units)
                .unwrap_or_default(),
        )?;
    }
    for record in &mut edge_records {
        record.version = CANON_ENTITY_EVIDENCE_VERSION_V1.to_string();
    }
    edge_records.sort_by(|left, right| {
        left.left_surface_id
            .cmp(&right.left_surface_id)
            .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
    });
    let artifact =
        build_edge_evidence_artifact_from_validated_block_contract(EdgeEvidenceArtifactRequest {
            block: block.artifact.clone(),
            strategy,
            edge_records_path: EDGE_RECORDS_PATH.to_string(),
            edge_records: edge_records.clone(),
            candidate_records: block.candidates.clone(),
            bucket_assertions: block.exact_buckets.clone(),
        })?;
    validate_edge_evidence_artifact_contract(&artifact)?;
    let (artifact, artifact_value) = publish_v1_stage_artifact(
        artifact,
        EntityArtifactStageV1::Evidence,
        &block.artifact_value,
        stage_strategy(base_strategy, "evidence"),
        artifact_reference_chain(&block.artifact_value)?,
    )?;
    let mut publication_files = vec![jsonl_publication_file(
        EDGE_RECORDS_PATH,
        "evidence",
        CANON_ENTITY_EVIDENCE_VERSION_V1,
        &edge_records,
    )?];
    if let Some(record_link_evidence) = &record_link_evidence {
        publication_files.push(EntityPublicationFileInput::new(
            RECORD_LINK_EVIDENCE_PATH,
            "evidence",
            CANON_EVIDENCE_VERSION,
            record_link_evidence.evidence_bytes.clone(),
        ));
        publication_files.push(EntityPublicationFileInput::new(
            ASSIGNMENT_ALIGNMENT_PATH,
            "evidence",
            ASSIGNMENT_ALIGNMENT_VERSION,
            record_link_evidence.alignment_bytes.clone(),
        ));
    }
    publication_files.push(json_publication_file(
        EDGE_ARTIFACT_PATH,
        "evidence",
        CANON_ENTITY_EVIDENCE_VERSION_V1,
        &artifact_value,
    )?);
    if mirror_stable_paths {
        let publication = publish_manual_stage_generation(
            request,
            &block.publication_context,
            vec![publication_upstream_ref(
                CANON_ENTITY_BLOCK_VERSION_V1,
                block.artifact.artifact_content_hash.clone(),
            )],
            &publication_files,
        )?;
        mirror_publication_files(request, &publication, &publication_files)?;
    }
    Ok((artifact, artifact_value, edge_records, publication_files))
}

fn build_record_link_evidence_run(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    block: &EntityBlockRun,
) -> Result<Option<RecordLinkEvidenceRun>, Refusal> {
    let Some(config) = &base_strategy.record_link else {
        return Ok(None);
    };
    let input_set = load_record_link_inputs(RecordLinkLoadRequest {
        workspace_root: strategy_workspace_root(request.strategy),
        sidecar_paths: config.input_paths.clone(),
        expected_profile_id: Some(block.artifact.metadata.profile.id.clone()),
        expected_profile_digest: block.artifact.metadata.profile.content_hash.clone(),
        expected_scope_id: None,
    })
    .map_err(|error| record_link_refusal(error, "evidence", request))?;
    let candidate_set = block.record_link_candidate_set.as_ref().ok_or_else(|| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Record-link evidence requires the block-stage candidate set",
            json!({
                "stage": "evidence",
                "record_link_stage": "record_link_evidence",
                "field": "record_link_candidate_set",
                "writes_performed": false
            }),
            Some(next_run_command(request)),
        )
    })?;
    let output = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set,
        feature_policies: &config.candidate_config.feature_policies,
        blocking_policy: config.candidate_config.blocking_policy.clone(),
        policy: config.assignment_alignment.clone(),
    })
    .map_err(|error| record_link_refusal(error, "evidence", request))?;
    let evidence_bytes = canonical_bundle_bytes(&output.bundle).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to serialize record-link evidence bundle",
            json!({
                "stage": "evidence",
                "record_link_stage": "record_link_evidence",
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(next_run_command(request)),
        )
    })?;
    let alignment_bytes = canonical_assignment_alignment_bytes(&output.alignment)
        .map_err(|error| record_link_refusal(error, "evidence", request))?;
    let mut edge_hits = output.edge_hits_by_surface_pair;
    for alignment in &output.alignment.alignments {
        if alignment.decision == AssignmentAlignmentDecisionKind::Aligned {
            edge_hits
                .entry(ordered_surface_pair(
                    &alignment.left.surface_id,
                    &alignment.right.surface_id,
                ))
                .or_default()
                .push(RecordLinkEdgeHit {
                    left_surface_id: alignment.left.surface_id.clone(),
                    right_surface_id: alignment.right.surface_id.clone(),
                    evidence_id: alignment.alignment_id.clone(),
                    lane: "relation_hint".to_string(),
                    hard_cannot_link: false,
                    score_units: config.assignment_hint_score_units,
                });
        }
    }
    for hits in edge_hits.values_mut() {
        hits.sort_by(|left, right| {
            left.evidence_id
                .cmp(&right.evidence_id)
                .then_with(|| left.lane.cmp(&right.lane))
        });
    }
    Ok(Some(RecordLinkEvidenceRun {
        evidence_bytes,
        alignment_bytes,
        edge_hits,
    }))
}

fn merge_record_link_edge_hits(
    edge_records: &mut Vec<EdgeEvidenceRecord>,
    edge_hits: &BTreeMap<(String, String), Vec<RecordLinkEdgeHit>>,
    assignment_hint_score_units: u64,
) -> Result<(), Refusal> {
    let mut by_pair = edge_records
        .drain(..)
        .map(|record| {
            (
                (
                    record.left_surface_id.clone(),
                    record.right_surface_id.clone(),
                ),
                record,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for ((left_surface_id, right_surface_id), hits) in edge_hits {
        if left_surface_id == right_surface_id {
            continue;
        }
        let mut merged_hits = by_pair
            .remove(&(left_surface_id.clone(), right_surface_id.clone()))
            .map(|record| record.hits)
            .unwrap_or_default();
        for hit in hits {
            merged_hits.push(record_link_edge_hit(hit, assignment_hint_score_units)?);
        }
        let mut rebuilt = build_edge_evidence_record(
            left_surface_id.clone(),
            right_surface_id.clone(),
            merged_hits,
        )?;
        rebuilt.version = CANON_ENTITY_EVIDENCE_VERSION_V1.to_string();
        by_pair.insert((left_surface_id.clone(), right_surface_id.clone()), rebuilt);
    }
    edge_records.extend(by_pair.into_values());
    Ok(())
}

fn record_link_edge_hit(
    hit: &RecordLinkEdgeHit,
    assignment_hint_score_units: u64,
) -> Result<EdgeEvidenceHit, Refusal> {
    let (lane, reason_code, score_units) = match hit.lane.as_str() {
        "support" => (
            ScoreLane::Support,
            "record_link_feature_support",
            hit.score_units,
        ),
        "anti_merge" => (
            ScoreLane::AntiMerge,
            "record_link_feature_conflict",
            hit.score_units,
        ),
        "relation_hint" => (
            ScoreLane::RelationHint,
            "record_link_assignment_alignment",
            if hit.score_units == 0 {
                assignment_hint_score_units
            } else {
                hit.score_units
            },
        ),
        _ => {
            return Err(EntityRefusalKind::ArtifactContract.to_refusal(
                "Record-link evidence emitted an unsupported edge lane",
                json!({
                    "stage": "evidence",
                    "record_link_stage": "record_link_evidence",
                    "lane": hit.lane,
                    "writes_performed": false
                }),
                None,
            ));
        }
    };
    Ok(EdgeEvidenceHit::new(
        lane,
        "record_link",
        format!("record_link:{}", hit.evidence_id),
        reason_code,
        ScoreUnits::saturating_from_units(score_units),
        hit.hard_cannot_link,
        format!(
            "record-link derived evidence_id={} left_surface_id={} right_surface_id={}",
            hit.evidence_id, hit.left_surface_id, hit.right_surface_id
        ),
    ))
}

struct SolveStageInput<'a> {
    edge: &'a EdgeEvidenceArtifact,
    edge_value: &'a Value,
    edge_records: &'a [EdgeEvidenceRecord],
    exact_buckets: &'a [ExactBucketAssertion],
    surfaces: &'a [PreparedSurfaceRecord],
}

fn build_and_write_solve(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    input: SolveStageInput<'_>,
    mirror_stable_paths: bool,
) -> Result<(SolveArtifact, Value, Vec<EntityPublicationFileInput>), Refusal> {
    let graph_edge_records = graph_edge_records(input.edge_records)?;
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: graph_edge_records,
        exact_bucket_assertions: input.exact_buckets.to_vec(),
        incumbent_ids: incumbent_ids(input.surfaces),
    })?;
    let mut metadata = input.edge.metadata.clone();
    metadata.strategy = stage_strategy(base_strategy, "solve");
    let mut upstream_artifacts = metadata.upstream_artifacts.clone();
    upstream_artifacts.push(EntityArtifactReference {
        version: input.edge.version.clone(),
        content_hash: input.edge.artifact_content_hash.clone(),
    });
    upstream_artifacts.sort_by(artifact_ref_cmp);
    upstream_artifacts.dedup();
    metadata.upstream_artifacts = upstream_artifacts;
    metadata.artifact_content_hash.clear();

    let artifact = build_solve_artifact_contract_with_alias_proposals(
        SolveArtifactRequest {
            metadata,
            graph,
            config: SolveReconciliationConfig::escrow_only(ScoreUnits::MAX),
            provenance: solve_provenance(input.surfaces),
            decision_ledger_path: DECISION_LEDGER_PATH.to_string(),
        },
        solve_alias_proposal_surfaces(input.surfaces),
    )?;
    validate_solve_artifact_contract(&artifact)?;
    let (artifact, artifact_value) = publish_v1_stage_artifact(
        artifact,
        EntityArtifactStageV1::Solve,
        input.edge_value,
        stage_strategy(base_strategy, "solve"),
        artifact_reference_chain(input.edge_value)?,
    )?;
    let publication_files = vec![
        EntityPublicationFileInput::new(
            DECISION_LEDGER_PATH,
            "solve",
            CANON_ENTITY_SOLVE_VERSION_V1,
            Vec::new(),
        ),
        json_publication_file(
            SOLVE_ARTIFACT_PATH,
            "solve",
            CANON_ENTITY_SOLVE_VERSION_V1,
            &artifact_value,
        )?,
    ];
    if mirror_stable_paths {
        let publication_context = read_cache_execution_receipt_context(request)?;
        let publication = publish_manual_stage_generation(
            request,
            &publication_context,
            vec![publication_upstream_ref(
                CANON_ENTITY_EVIDENCE_VERSION_V1,
                input.edge.artifact_content_hash.clone(),
            )],
            &publication_files,
        )?;
        mirror_publication_files(request, &publication, &publication_files)?;
    }
    Ok((artifact, artifact_value, publication_files))
}

fn graph_edge_records(
    edge_records: &[EdgeEvidenceRecord],
) -> Result<Vec<EdgeEvidenceRecord>, Refusal> {
    edge_records
        .iter()
        .map(|record| {
            build_edge_evidence_record(
                record.left_surface_id.clone(),
                record.right_surface_id.clone(),
                record.hits.clone(),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn run_artifact(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    prepare: &PrepareRunArtifact,
    prepare_value: &Value,
    surfaces: &[PreparedSurfaceRecord],
    index: &EntityIndexRun,
    block: &BlockCandidateArtifact,
    block_value: &Value,
    edge: &EdgeEvidenceArtifact,
    edge_value: &Value,
    solve: &SolveArtifact,
    solve_value: &Value,
) -> Result<(EntityRunArtifact, Value), Refusal> {
    let stage_artifacts = stage_artifacts(prepare, index, block, edge, solve);
    let mut metadata = solve.metadata.clone();
    metadata.strategy = EntityStrategyReference {
        id: base_strategy.id.clone(),
        version: base_strategy.version.clone(),
        content_hash: base_strategy.content_hash.clone(),
    };
    metadata.upstream_artifacts = stage_artifacts
        .iter()
        .map(|stage| EntityArtifactReference {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .collect();
    metadata.artifact_content_hash.clear();
    let orchestration = run_orchestration(request, &stage_artifacts, prepare, solve);

    let artifact = EntityRunArtifact {
        version: CANON_ENTITY_RUN_VERSION_V1.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary: run_summary(request, prepare, surfaces, index, block, edge, solve),
        orchestration,
        stage_artifacts,
        work_dir: EntityRunWorkDirLayout {
            prepare_artifact_path: PREPARE_ARTIFACT_PATH.to_string(),
            surfaces_path: prepare.surfaces_path.clone(),
            index_artifact_path: "index/index.json".to_string(),
            block_artifact_path: BLOCK_ARTIFACT_PATH.to_string(),
            candidate_records_path: BLOCK_CANDIDATES_PATH.to_string(),
            candidate_diagnostics_path: BLOCK_DIAGNOSTICS_PATH.to_string(),
            exact_bucket_assertions_path: BLOCK_EXACT_BUCKETS_PATH.to_string(),
            edge_artifact_path: EDGE_ARTIFACT_PATH.to_string(),
            edge_records_path: EDGE_RECORDS_PATH.to_string(),
            solve_artifact_path: SOLVE_ARTIFACT_PATH.to_string(),
            decision_ledger_path: DECISION_LEDGER_PATH.to_string(),
            run_artifact_path: RUN_ARTIFACT_PATH.to_string(),
        },
        next_commands: EntityRunNextCommands {
            resume: next_run_command(request),
            review_export: format!(
                "canon entity review export {} --include escrow --emit csv",
                request.work_dir.join(SOLVE_ARTIFACT_PATH).display()
            ),
            audit: format!(
                "canon entity audit {} --suite <SUITE_DIR>",
                request.work_dir.join(SOLVE_ARTIFACT_PATH).display()
            ),
            promote: format!(
                "canon entity promote {} --audit {} --registry {} --next-version <VERSION>",
                request.work_dir.join(SOLVE_ARTIFACT_PATH).display(),
                request.work_dir.join("audit.json").display(),
                request.registry.display()
            ),
            apply: format!(
                "canon entity apply {} --registry {} --column <COLUMN> --out <OUT>",
                request.rows.display(),
                request.registry.display()
            ),
        },
    };
    let upstreams = sort_entity_v1_upstream_references(vec![
        entity_v1_artifact_reference(prepare_value)?,
        entity_v1_artifact_reference(&index.artifact_value)?,
        entity_v1_artifact_reference(block_value)?,
        entity_v1_artifact_reference(edge_value)?,
        entity_v1_artifact_reference(solve_value)?,
    ])?;
    publish_v1_stage_artifact(
        artifact,
        EntityArtifactStageV1::Run,
        solve_value,
        EntityStrategyReference {
            id: base_strategy.id.clone(),
            version: base_strategy.version.clone(),
            content_hash: base_strategy.content_hash.clone(),
        },
        upstreams,
    )
}

fn run_manifest(artifact: &EntityRunArtifact) -> Value {
    json!({
        "version": "canon_entity_run_manifest.v0",
        "summary": artifact.summary,
        "stage_artifacts": artifact.stage_artifacts,
        "orchestration": artifact.orchestration,
        "next_commands": artifact.next_commands
    })
}

fn run_publication_files(
    artifact: &EntityRunArtifact,
    artifact_value: &Value,
) -> Result<Vec<EntityPublicationFileInput>, Refusal> {
    Ok(vec![
        json_publication_file(
            RUN_MANIFEST_PATH,
            "run",
            "canon_entity_run_manifest.v0",
            &run_manifest(artifact),
        )?,
        json_publication_file(
            RUN_ARTIFACT_PATH,
            "run",
            CANON_ENTITY_RUN_VERSION_V1,
            artifact_value,
        )?,
    ])
}

fn publish_run_stage_generation(
    request: EntityRunRequest<'_>,
    index: &EntityIndexRun,
    stage_artifacts: &[EntityRunStageArtifact],
    files: &[EntityPublicationFileInput],
) -> Result<EntityRunPublicationResult, Refusal> {
    let upstream_artifacts = stage_artifacts
        .iter()
        .map(|stage| EntityPublicationUpstreamRef {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .collect::<Vec<_>>();
    publish_stage_generation(
        request,
        index.cache_mode,
        index.cache_status,
        &index.cache_execution_receipt_content_hash,
        upstream_artifacts,
        files,
    )
}

fn publish_manual_stage_generation(
    request: EntityRunRequest<'_>,
    context: &EntityRunPublicationContext,
    upstream_artifacts: Vec<EntityPublicationUpstreamRef>,
    files: &[EntityPublicationFileInput],
) -> Result<EntityRunPublicationResult, Refusal> {
    publish_stage_generation(
        request,
        context.cache_mode,
        context.cache_status,
        &context.cache_receipt_hash,
        upstream_artifacts,
        files,
    )
}

fn publish_stage_generation(
    request: EntityRunRequest<'_>,
    cache_mode: EntityIndexCacheMode,
    cache_status: EntityIndexCacheStatus,
    cache_receipt_hash: &str,
    upstream_artifacts: Vec<EntityPublicationUpstreamRef>,
    files: &[EntityPublicationFileInput],
) -> Result<EntityRunPublicationResult, Refusal> {
    publish_stage_generation_at_work_dir(
        request.work_dir,
        Some(next_run_command(request)),
        None,
        EntityRunPublicationCacheInput {
            mode: cache_mode.as_str(),
            status: cache_status.as_str(),
            receipt_hash: cache_receipt_hash,
        },
        upstream_artifacts,
        files,
    )
}

struct EntityRunPublicationCacheInput<'a> {
    mode: &'a str,
    status: &'a str,
    receipt_hash: &'a str,
}

fn publish_stage_generation_at_work_dir(
    work_dir: &Path,
    next_command: Option<String>,
    supersedes_generation_id: Option<String>,
    cache: EntityRunPublicationCacheInput<'_>,
    upstream_artifacts: Vec<EntityPublicationUpstreamRef>,
    files: &[EntityPublicationFileInput],
) -> Result<EntityRunPublicationResult, Refusal> {
    let omit_logical_paths =
        publication_omitted_logical_paths_for_patch(work_dir, files, next_command.clone())?;
    let publication_request = EntityPublicationRequest {
        stream_id: RUN_PUBLICATION_STREAM_ID.to_string(),
        supersedes_generation_id,
        request_fingerprint: run_publication_request_fingerprint(
            cache.mode,
            cache.status,
            cache.receipt_hash,
            &upstream_artifacts,
            &omit_logical_paths,
            files,
        )?,
        cache_mode: cache.mode.to_string(),
        cache_status: cache.status.to_string(),
        cache_receipt_hash: cache.receipt_hash.to_string(),
        stage_order: run_publication_stage_order(),
        upstream_artifacts,
        files: files.to_vec(),
        omit_logical_paths,
    };
    let receipt = publish_stream_patch(work_dir, publication_request)
        .map_err(|error| publication_refusal_with_next_command(error, next_command.clone()))?;
    Ok(publication_result(receipt))
}

fn publication_omitted_logical_paths_for_patch(
    work_dir: &Path,
    files: &[EntityPublicationFileInput],
    next_command: Option<String>,
) -> Result<Vec<String>, Refusal> {
    let Some(max_patch_stage_rank) = files
        .iter()
        .map(|file| publication_stage_rank(&file.stage))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
    else {
        return Ok(Vec::new());
    };
    let current = match open_current_stream_generation(work_dir, RUN_PUBLICATION_STREAM_ID) {
        Ok(snapshot) => snapshot,
        Err(error) if is_absent_publication_stream(&error) => return Ok(Vec::new()),
        Err(error) => {
            return Err(publication_refusal_with_next_command(
                error,
                next_command.clone(),
            ));
        }
    };
    let patch_paths = files
        .iter()
        .map(|file| file.logical_path.clone())
        .collect::<BTreeSet<_>>();
    let mut omitted = Vec::new();
    for file in &current.manifest.files {
        if patch_paths.contains(&file.logical_path) {
            continue;
        }
        let rank = publication_stage_rank(&file.stage)?;
        if rank >= max_patch_stage_rank {
            omitted.push(file.logical_path.clone());
        }
    }
    omitted.sort();
    Ok(omitted)
}

fn publication_stage_rank(stage: &str) -> Result<u8, Refusal> {
    match stage {
        "block" => Ok(0),
        "evidence" => Ok(1),
        "solve" => Ok(2),
        "run" => Ok(3),
        "link" => Ok(4),
        _ => Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Unsupported entity run publication stage",
            json!({
                "stage": "run",
                "publication_stage": "entity_run_stage_set",
                "file_stage": stage,
                "writes_performed": false
            }),
            None,
        )),
    }
}

fn run_publication_stage_order() -> Vec<String> {
    vec![
        "block".to_string(),
        "evidence".to_string(),
        "solve".to_string(),
        "run".to_string(),
        "link".to_string(),
    ]
}

fn run_publication_request_fingerprint(
    cache_mode: &str,
    cache_status: &str,
    cache_receipt_hash: &str,
    upstream_artifacts: &[EntityPublicationUpstreamRef],
    omit_logical_paths: &[String],
    files: &[EntityPublicationFileInput],
) -> Result<String, Refusal> {
    let mut file_refs = files
        .iter()
        .map(|file| {
            json!({
                "logical_path": file.logical_path.as_str(),
                "stage": file.stage.as_str(),
                "version": file.version.as_str(),
                "byte_len": file.bytes.len(),
                "content_hash": witness::hash_bytes(&file.bytes)
            })
        })
        .collect::<Vec<_>>();
    file_refs.sort_by(|left, right| {
        left["logical_path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["logical_path"].as_str().unwrap_or_default())
            .then_with(|| {
                left["stage"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["stage"].as_str().unwrap_or_default())
            })
    });
    let mut stage_refs = upstream_artifacts
        .iter()
        .map(|reference| {
            json!({
                "version": reference.version.as_str(),
                "artifact_content_hash": reference.content_hash.as_str()
            })
        })
        .collect::<Vec<_>>();
    stage_refs.sort_by(|left, right| {
        left["version"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["version"].as_str().unwrap_or_default())
            .then_with(|| {
                left["artifact_content_hash"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["artifact_content_hash"].as_str().unwrap_or_default())
            })
    });
    let bytes = serde_json::to_vec(&json!({
        "version": RUN_PUBLICATION_REQUEST_VERSION,
        "stream_id": RUN_PUBLICATION_STREAM_ID,
        "cache_mode": cache_mode,
        "cache_status": cache_status,
        "cache_receipt_hash": cache_receipt_hash,
        "upstream_artifacts": stage_refs,
        "omit_logical_paths": omit_logical_paths,
        "files": file_refs
    }))
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to serialize entity run publication request fingerprint",
            json!({
                "stage": "run",
                "stream_id": RUN_PUBLICATION_STREAM_ID,
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn publication_result(receipt: EntityPublicationReceipt) -> EntityRunPublicationResult {
    EntityRunPublicationResult {
        stream_id: RUN_PUBLICATION_STREAM_ID.to_string(),
        generation_id: receipt.generation_id,
        outcome: publication_outcome(receipt.outcome).to_string(),
        writes_performed: receipt.writes_performed,
        committed: receipt.committed,
        manifest_path: receipt.manifest_path,
        commit_marker_path: receipt.commit_marker_path,
        object_count: receipt.object_count,
    }
}

fn publication_outcome(outcome: EntityPublicationOutcome) -> &'static str {
    match outcome {
        EntityPublicationOutcome::Committed => "committed",
        EntityPublicationOutcome::AlreadyCommitted => "already_committed",
        EntityPublicationOutcome::CommitUnknown => "commit_unknown",
    }
}

fn publication_refusal_with_next_command(
    error: EntityPublicationError,
    next_command: Option<String>,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Failed to publish entity run stage generation",
        json!({
            "stage": "run",
            "publication_stage": "entity_run_stage_set",
            "stream_id": RUN_PUBLICATION_STREAM_ID,
            "error_kind": format!("{:?}", error.kind),
            "error": error.message,
            "writes_performed": error.writes_performed,
            "committed": error.committed,
            "generation_id": error.generation_id
        }),
        next_command,
    )
}

fn run_orchestration(
    request: EntityRunRequest<'_>,
    stage_artifacts: &[EntityRunStageArtifact],
    prepare: &PrepareRunArtifact,
    solve: &SolveArtifact,
) -> EntityRunOrchestration {
    let solve_ref = EntityArtifactReference {
        version: solve.version.clone(),
        content_hash: solve.artifact_content_hash.clone(),
    };
    let stage_refs = stage_artifacts
        .iter()
        .map(|stage| EntityArtifactReference {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .collect::<Vec<_>>();
    let solve_path = request
        .work_dir
        .join(SOLVE_ARTIFACT_PATH)
        .display()
        .to_string();
    let review_path = request.work_dir.join("review.csv").display().to_string();
    let audit_path = request.work_dir.join("audit.json").display().to_string();
    let promote_path = request.work_dir.join("promote.json").display().to_string();
    let sidecar_path = request
        .work_dir
        .join("promotion-sidecars.json")
        .display()
        .to_string();
    let apply_path = request.work_dir.join("apply.csv").display().to_string();

    EntityRunOrchestration {
        stage_order: stage_artifacts
            .iter()
            .map(|stage| stage.stage.clone())
            .chain(
                [
                    "review_export",
                    "audit",
                    "review_import",
                    "promote",
                    "apply",
                ]
                .into_iter()
                .map(str::to_string),
            )
            .collect(),
        profile_firewall: EntityRunProfileFirewall {
            profile_id: prepare.profile.id.clone(),
            profile_version: prepare.profile.version.clone(),
            identity_semantics: prepare.profile.identity_semantics.clone(),
            canonical_type: prepare.profile.canonical_type.clone(),
            registry_id: prepare.registry_snapshot.id.clone(),
            registry_version: prepare.registry_snapshot.version.clone(),
            registry_snapshot_hash: prepare.registry_snapshot.lookup_snapshot_hash.clone(),
            sidecar_snapshot_hash: solve
                .metadata
                .registry_snapshot
                .sidecar_snapshot_hash
                .clone(),
            strategy_hash: solve.metadata.strategy.content_hash.clone(),
        },
        handoff_steps: vec![
            EntityRunHandoffStep {
                stage: "review_export".to_string(),
                command: format!(
                    "canon entity review export {solve_path} --include escrow --emit csv > {review_path}"
                ),
                input_artifact_path: solve_path.clone(),
                input_artifacts: vec![solve_ref.clone()],
                output_paths: vec![review_path.clone()],
                required_prior_stages: vec!["solve".to_string()],
                requires_audit: false,
                enforces_profile_firewall: true,
                ..EntityRunHandoffStep::default()
            },
            EntityRunHandoffStep {
                stage: "audit".to_string(),
                command: format!(
                    "canon entity audit {solve_path} --suite <SUITE_DIR> > {audit_path}"
                ),
                input_artifact_path: solve_path.clone(),
                input_artifacts: stage_refs,
                output_paths: vec![audit_path.clone()],
                required_prior_stages: vec!["solve".to_string()],
                requires_audit: false,
                enforces_profile_firewall: true,
                ..EntityRunHandoffStep::default()
            },
            EntityRunHandoffStep {
                stage: "review_import".to_string(),
                command: format!(
                    "canon entity review import {review_path} --registry {} --next-version <VERSION> --audit {audit_path}",
                    request.registry.display()
                ),
                input_artifact_path: review_path.clone(),
                input_artifacts: vec![solve_ref.clone()],
                required_paths: vec![review_path.clone(), audit_path.clone()],
                output_paths: vec![
                    request
                        .work_dir
                        .join(DECISION_LEDGER_PATH)
                        .display()
                        .to_string(),
                ],
                required_prior_stages: vec!["review_export".to_string(), "audit".to_string()],
                requires_audit: true,
                enforces_profile_firewall: true,
            },
            EntityRunHandoffStep {
                stage: "promote".to_string(),
                command: format!(
                    "canon entity promote {solve_path} --audit {audit_path} --registry {} --next-version <VERSION> > {promote_path}",
                    request.registry.display()
                ),
                input_artifact_path: solve_path.clone(),
                input_artifacts: vec![solve_ref.clone()],
                required_paths: vec![audit_path.clone()],
                output_paths: vec![promote_path, sidecar_path.clone()],
                required_prior_stages: vec!["audit".to_string(), "review_import".to_string()],
                requires_audit: true,
                enforces_profile_firewall: true,
            },
            EntityRunHandoffStep {
                stage: "apply".to_string(),
                command: format!(
                    "canon entity apply {} --registry {} --column <COLUMN> --out {apply_path}",
                    request.rows.display(),
                    request.registry.display()
                ),
                input_artifact_path: request.rows.display().to_string(),
                input_artifacts: vec![solve_ref],
                required_paths: vec![request.registry.display().to_string(), sidecar_path],
                output_paths: vec![apply_path],
                required_prior_stages: vec!["promote".to_string()],
                requires_audit: true,
                enforces_profile_firewall: true,
            },
        ],
    }
}

fn run_summary(
    request: EntityRunRequest<'_>,
    prepare: &PrepareRunArtifact,
    surfaces: &[PreparedSurfaceRecord],
    index: &EntityIndexRun,
    block: &BlockCandidateArtifact,
    edge: &EdgeEvidenceArtifact,
    solve: &SolveArtifact,
) -> EntityDeterministicSummary {
    let exact_resolved_surfaces = surfaces
        .iter()
        .filter(|surface| surface.exact_lookup.status == PreparedExactLookupStatus::Resolved)
        .count() as u64;
    EntityDeterministicSummary {
        counts: BTreeMap::from([
            ("row_count".to_string(), prepare.input.row_count),
            (
                "prepared_surfaces".to_string(),
                surfaces.len().try_into().expect("surface count fits u64"),
            ),
            (
                "physical_batch_count".to_string(),
                prepare.streaming.telemetry.chunk_count,
            ),
            (
                "max_physical_batch_rows".to_string(),
                prepare.streaming.telemetry.max_chunk_rows,
            ),
            (
                "exact_resolved_surfaces".to_string(),
                exact_resolved_surfaces,
            ),
            (
                "index_surfaces".to_string(),
                count(&index.artifact.summary.counts, "surface_count"),
            ),
            (
                "exact_bucket_count".to_string(),
                count(&block.summary.counts, "exact_bucket_count"),
            ),
            (
                "candidate_pairs".to_string(),
                count(&block.summary.counts, "candidate_pairs"),
            ),
            (
                "evidence_records".to_string(),
                count(&edge.summary.counts, "evidence_records"),
            ),
            (
                "relation_hint_evidence".to_string(),
                count(&edge.summary.counts, "relation_hint_count"),
            ),
            (
                "solved_entities".to_string(),
                count(&solve.summary.counts, "entity_count"),
            ),
            (
                "review_group_count".to_string(),
                count(&solve.summary.counts, "review_group_count"),
            ),
        ]),
        labels: BTreeMap::from([
            ("profile_id".to_string(), prepare.profile.id.clone()),
            (
                "profile_version".to_string(),
                prepare.profile.version.clone(),
            ),
            (
                "registry_id".to_string(),
                prepare.registry_snapshot.id.clone(),
            ),
            (
                "registry_version".to_string(),
                prepare.registry_snapshot.version.clone(),
            ),
            (
                "registry_source".to_string(),
                request.registry.display().to_string(),
            ),
            (
                "strategy_source".to_string(),
                request.strategy.display().to_string(),
            ),
            (
                "cache_mode".to_string(),
                index.cache_mode.as_str().to_string(),
            ),
            (
                "cache_status".to_string(),
                index.cache_status.as_str().to_string(),
            ),
            (
                "cache_receipt_path".to_string(),
                index.cache_execution_receipt_path.clone(),
            ),
            (
                "cache_receipt_hash".to_string(),
                index.cache_execution_receipt_content_hash.clone(),
            ),
            (
                "cache_bundle_receipt_path".to_string(),
                index.cache_bundle_receipt_path.clone(),
            ),
            (
                "cache_bundle_receipt_hash".to_string(),
                index.cache_bundle_receipt_content_hash.clone(),
            ),
            (
                "batching_mode".to_string(),
                if prepare.streaming.telemetry.chunk_count > 1 {
                    "physical_batches"
                } else {
                    "single_batch"
                }
                .to_string(),
            ),
            ("status".to_string(), "completed".to_string()),
        ]),
    }
}

struct EdgeSupportScoringContext<'a> {
    profile: &'a EntityProfileDocument,
    support_namespace: &'a str,
    relation_namespace: &'a str,
    surface_lookup: BTreeMap<&'a str, &'a PreparedSurfaceRecord>,
    tfidf_models: BTreeMap<String, SparseTfidfModel>,
}

impl<'a> EdgeSupportScoringContext<'a> {
    fn new(
        profile: &'a EntityProfileDocument,
        support_namespace: &'a str,
        relation_namespace: &'a str,
        surfaces: &'a [PreparedSurfaceRecord],
    ) -> Result<Self, Refusal> {
        validate_support_operator_params(profile)?;
        let surface_lookup = surfaces
            .iter()
            .map(|surface| (surface.surface_id.as_str(), surface))
            .collect::<BTreeMap<_, _>>();
        let tfidf_models = tfidf_models_for_profile(profile, surfaces)?;
        Ok(Self {
            profile,
            support_namespace,
            relation_namespace,
            surface_lookup,
            tfidf_models,
        })
    }
}

pub(crate) fn score_edge_candidate_for_prepared_surfaces(
    profile: &EntityProfileDocument,
    support_namespace: &str,
    relation_namespace: &str,
    surfaces: &[PreparedSurfaceRecord],
    candidate: &BlockCandidateRecord,
) -> Result<EdgeEvidenceRecord, Refusal> {
    let context =
        EdgeSupportScoringContext::new(profile, support_namespace, relation_namespace, surfaces)?;
    edge_record_for_candidate(candidate, &context)
}

fn validate_edge_profile_binding(
    loaded_profile: &LoadedPrepareProfile,
    expected: &crate::entity::EntityProfileReference,
) -> Result<(), Refusal> {
    let mut actual = loaded_profile.document.to_reference();
    actual.content_hash = Some(loaded_profile.content_hash.clone());
    if actual != *expected {
        return Err(stage_context_refusal(
            "edge",
            "metadata.profile",
            json!(expected),
            json!(actual),
        ));
    }
    Ok(())
}

fn edge_record_for_candidate(
    candidate: &crate::entity::block::BlockCandidateRecord,
    context: &EdgeSupportScoringContext<'_>,
) -> Result<EdgeEvidenceRecord, Refusal> {
    let hits = support_hits_for_candidate(candidate, context)?;
    if hits.is_empty() {
        relation_hint_edge(
            candidate,
            context.relation_namespace,
            &context.surface_lookup,
        )
    } else {
        build_edge_evidence_record(
            candidate.left_surface_id.clone(),
            candidate.right_surface_id.clone(),
            hits,
        )
    }
}

fn support_hits_for_candidate(
    candidate: &crate::entity::block::BlockCandidateRecord,
    context: &EdgeSupportScoringContext<'_>,
) -> Result<Vec<EdgeEvidenceHit>, Refusal> {
    let left = candidate_surface(&candidate.left_surface_id, context)?;
    let right = candidate_surface(&candidate.right_surface_id, context)?;
    let mut hits = Vec::new();

    for spec in &context.profile.evidence.support {
        let hit = match spec.op.as_str() {
            "exact_view" => exact_view_support_for_spec(spec, left, right, context)?,
            "string_similarity" => string_similarity_support_for_spec(spec, left, right, context)?,
            "tfidf_cosine" => tfidf_support_for_spec(spec, candidate, context)?,
            _ => None,
        };
        if let Some(hit) = hit {
            hits.push(hit);
        }
    }

    Ok(hits)
}

fn candidate_surface<'a>(
    surface_id: &str,
    context: &'a EdgeSupportScoringContext<'a>,
) -> Result<&'a PreparedSurfaceRecord, Refusal> {
    context
        .surface_lookup
        .get(surface_id)
        .copied()
        .ok_or_else(|| {
            stage_context_refusal(
                "edge",
                "candidate_records.surface_id",
                json!("known prepared surface"),
                json!(surface_id),
            )
        })
}

fn exact_view_support_for_spec(
    spec: &EntityOperatorSpec,
    left: &PreparedSurfaceRecord,
    right: &PreparedSurfaceRecord,
    context: &EdgeSupportScoringContext<'_>,
) -> Result<Option<EdgeEvidenceHit>, Refusal> {
    let Some(view_name) = support_view_name(spec)? else {
        return Ok(None);
    };
    let score_units =
        optional_score_units_param(spec, "score_units", "score")?.unwrap_or(ScoreUnits::MAX);
    if score_units == ScoreUnits::ZERO {
        return Ok(None);
    }
    let operator_id = support_operator_id(spec);
    Ok(exact_view_support_hit(ExactViewSupportRequest {
        namespace: context.support_namespace,
        operator_id: &operator_id,
        reason_code: "exact_view_support",
        view_name,
        left_value: support_view_value(left, view_name, "exact_view")?,
        right_value: support_view_value(right, view_name, "exact_view")?,
        score_units,
    }))
}

fn string_similarity_support_for_spec(
    spec: &EntityOperatorSpec,
    left: &PreparedSurfaceRecord,
    right: &PreparedSurfaceRecord,
    context: &EdgeSupportScoringContext<'_>,
) -> Result<Option<EdgeEvidenceHit>, Refusal> {
    let Some(score_cutoff) = positive_support_threshold(spec)? else {
        return Ok(None);
    };
    let view_name = required_support_view_name(spec, "string_similarity")?;
    let metric = required_similarity_metric(spec)?;
    let score_hint = optional_score_units_param(spec, "score_hint_units", "score_hint")?;
    let operator_id = support_operator_id(spec);
    Ok(string_similarity_support_hit(
        StringSimilaritySupportRequest {
            namespace: context.support_namespace,
            operator_id: &operator_id,
            reason_code: "string_similarity_support",
            metric,
            left_value: support_view_value(left, view_name, "string_similarity")?,
            right_value: support_view_value(right, view_name, "string_similarity")?,
            score_cutoff: Some(score_cutoff),
            score_hint,
        },
    ))
}

fn tfidf_support_for_spec(
    spec: &EntityOperatorSpec,
    candidate: &crate::entity::block::BlockCandidateRecord,
    context: &EdgeSupportScoringContext<'_>,
) -> Result<Option<EdgeEvidenceHit>, Refusal> {
    let Some(min_score_units) = positive_support_threshold(spec)? else {
        return Ok(None);
    };
    let view_name = required_support_view_name(spec, "tfidf_cosine")?;
    let model = context.tfidf_models.get(view_name).ok_or_else(|| {
        edge_support_config_refusal(
            "TF-IDF support model is missing for profile-declared evidence view",
            "tfidf_cosine",
            "view",
            json!({ "view": view_name }),
        )
    })?;
    let operator_id = support_operator_id(spec);
    Ok(tfidf_cosine_support_hit(TfidfCosineSupportRequest {
        namespace: context.support_namespace,
        operator_id: &operator_id,
        model,
        left_surface_id: &candidate.left_surface_id,
        right_surface_id: &candidate.right_surface_id,
        min_score_units,
        top_k: positive_usize_param(spec, "top_k", 25)?,
        candidate_cap: Some(positive_usize_param(spec, "candidate_cap", 25)?),
    }))
}

fn validate_support_operator_params(profile: &EntityProfileDocument) -> Result<(), Refusal> {
    for spec in &profile.evidence.support {
        match spec.op.as_str() {
            "string_similarity" => {
                if positive_support_threshold(spec)?.is_some() {
                    required_support_view_name(spec, "string_similarity")?;
                    required_similarity_metric(spec)?;
                    optional_score_units_param(spec, "score_hint_units", "score_hint")?;
                }
            }
            "tfidf_cosine" => {
                if positive_support_threshold(spec)?.is_some() {
                    required_support_view_name(spec, "tfidf_cosine")?;
                    positive_usize_param(spec, "top_k", 25)?;
                    positive_usize_param(spec, "candidate_cap", 25)?;
                }
            }
            "exact_view" => {
                optional_score_units_param(spec, "score_units", "score")?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn tfidf_models_for_profile(
    profile: &EntityProfileDocument,
    surfaces: &[PreparedSurfaceRecord],
) -> Result<BTreeMap<String, SparseTfidfModel>, Refusal> {
    let mut views = BTreeSet::new();
    for spec in &profile.evidence.support {
        if spec.op == "tfidf_cosine" && positive_support_threshold(spec)?.is_some() {
            views.insert(required_support_view_name(spec, "tfidf_cosine")?.to_string());
        }
    }

    let mut models = BTreeMap::new();
    for view_name in views {
        models.insert(
            view_name.clone(),
            tfidf_model_for_view(&view_name, surfaces)?,
        );
    }
    Ok(models)
}

fn tfidf_model_for_view(
    view_name: &str,
    surfaces: &[PreparedSurfaceRecord],
) -> Result<SparseTfidfModel, Refusal> {
    let inputs = surfaces
        .iter()
        .map(|surface| {
            let value = support_view_value(surface, view_name, "tfidf_cosine")?;
            Ok(TfidfInputSurface::tokenized(
                surface.surface_id.clone(),
                value.to_string(),
                value.split_whitespace().map(ToOwned::to_owned),
            ))
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    Ok(SparseTfidfModel::build(&inputs))
}

fn support_view_name(spec: &EntityOperatorSpec) -> Result<Option<&str>, Refusal> {
    match spec.view.as_deref().map(str::trim) {
        Some("") => Err(edge_support_config_refusal(
            "Profile-declared support evidence view must be non-empty",
            &spec.op,
            "view",
            json!({ "view": spec.view.as_deref() }),
        )),
        Some(view_name) => Ok(Some(view_name)),
        None => Ok(None),
    }
}

fn required_support_view_name<'a>(
    spec: &'a EntityOperatorSpec,
    operator: &'static str,
) -> Result<&'a str, Refusal> {
    support_view_name(spec)?.ok_or_else(|| {
        edge_support_config_refusal(
            "Profile-declared support evidence requires an explicit view",
            operator,
            "view",
            json!({ "operator": operator }),
        )
    })
}

fn support_view_value<'a>(
    surface: &'a PreparedSurfaceRecord,
    view_name: &str,
    operator: &'static str,
) -> Result<&'a str, Refusal> {
    surface
        .normalized_views
        .get(view_name)
        .map(|view| view.value.as_str())
        .ok_or_else(|| {
            edge_support_config_refusal(
                "Prepared surface is missing the profile-declared evidence view",
                operator,
                "view",
                json!({
                    "view": view_name,
                    "surface_id": surface.surface_id.as_str(),
                    "available_views": surface.normalized_views.keys().cloned().collect::<Vec<_>>()
                }),
            )
        })
}

fn positive_support_threshold(spec: &EntityOperatorSpec) -> Result<Option<ScoreUnits>, Refusal> {
    let threshold = optional_score_units_param(spec, "min_score_units", "min_score")?;
    Ok(threshold.filter(|score_units| *score_units > ScoreUnits::ZERO))
}

fn optional_score_units_param(
    spec: &EntityOperatorSpec,
    units_key: &'static str,
    decimal_key: &'static str,
) -> Result<Option<ScoreUnits>, Refusal> {
    if let Some(value) = spec.params.get(units_key) {
        return parse_score_units_param(value, &spec.op, units_key).map(Some);
    }
    if let Some(value) = spec.params.get(decimal_key) {
        return parse_decimal_score_param(value, &spec.op, decimal_key).map(Some);
    }
    Ok(None)
}

fn parse_score_units_param(
    value: &str,
    operator: &str,
    field: &'static str,
) -> Result<ScoreUnits, Refusal> {
    let units = value.trim().parse::<u32>().map_err(|_| {
        edge_support_config_refusal(
            "Profile-declared score threshold must be an integer score unit",
            operator,
            field,
            json!({ "value": value }),
        )
    })?;
    ScoreUnits::from_scaled(units).ok_or_else(|| {
        edge_support_config_refusal(
            "Profile-declared score threshold is outside the entity score scale",
            operator,
            field,
            json!({ "value": value, "max": ScoreUnits::MAX.as_u32() }),
        )
    })
}

fn parse_decimal_score_param(
    value: &str,
    operator: &str,
    field: &'static str,
) -> Result<ScoreUnits, Refusal> {
    let trimmed = value.trim();
    let Some((whole, fractional)) = trimmed.split_once('.') else {
        return match trimmed {
            "0" => Ok(ScoreUnits::ZERO),
            "1" => Ok(ScoreUnits::MAX),
            _ => parse_score_units_param(trimmed, operator, field),
        };
    };
    if !matches!(whole, "0" | "1")
        || fractional.is_empty()
        || fractional.len() > 4
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(edge_support_config_refusal(
            "Profile-declared decimal score threshold must be between 0 and 1 with at most four fractional digits",
            operator,
            field,
            json!({ "value": value }),
        ));
    }
    let mut fractional_units = fractional.parse::<u32>().map_err(|_| {
        edge_support_config_refusal(
            "Profile-declared decimal score threshold is malformed",
            operator,
            field,
            json!({ "value": value }),
        )
    })?;
    for _ in fractional.len()..4 {
        fractional_units *= 10;
    }
    let whole_units = if whole == "1" {
        ScoreUnits::MAX.as_u32()
    } else {
        0
    };
    let units = whole_units.saturating_add(fractional_units);
    ScoreUnits::from_scaled(units).ok_or_else(|| {
        edge_support_config_refusal(
            "Profile-declared decimal score threshold is outside the entity score scale",
            operator,
            field,
            json!({ "value": value, "max": "1.0" }),
        )
    })
}

fn positive_usize_param(
    spec: &EntityOperatorSpec,
    field: &'static str,
    default: usize,
) -> Result<usize, Refusal> {
    let Some(value) = spec.params.get(field) else {
        return Ok(default);
    };
    let parsed = value.trim().parse::<usize>().map_err(|_| {
        edge_support_config_refusal(
            "Profile-declared support evidence parameter must be a positive integer",
            &spec.op,
            field,
            json!({ "value": value }),
        )
    })?;
    if parsed == 0 {
        return Err(edge_support_config_refusal(
            "Profile-declared support evidence parameter must be positive",
            &spec.op,
            field,
            json!({ "value": value }),
        ));
    }
    Ok(parsed)
}

fn required_similarity_metric(spec: &EntityOperatorSpec) -> Result<SimilarityMetric, Refusal> {
    let metric = spec.params.get("metric").ok_or_else(|| {
        edge_support_config_refusal(
            "Profile-declared string similarity support requires a metric",
            &spec.op,
            "metric",
            json!({ "operator": spec.op.as_str() }),
        )
    })?;
    match metric.trim() {
        "levenshtein_normalized" => Ok(SimilarityMetric::LevenshteinNormalized),
        "jaro_winkler" => Ok(SimilarityMetric::JaroWinkler),
        "dice_sorensen" => Ok(SimilarityMetric::DiceSorensen),
        "token_sort_ratio" => Ok(SimilarityMetric::TokenSortRatio),
        "token_set_ratio" => Ok(SimilarityMetric::TokenSetRatio),
        _ => Err(edge_support_config_refusal(
            "Profile-declared string similarity metric is unsupported",
            &spec.op,
            "metric",
            json!({ "value": metric }),
        )),
    }
}

fn support_operator_id(spec: &EntityOperatorSpec) -> String {
    spec.view
        .as_deref()
        .map(|view_name| format!("{}:{view_name}", spec.op))
        .unwrap_or_else(|| spec.op.clone())
}

fn edge_support_config_refusal(
    message: &'static str,
    operator: &str,
    field: &'static str,
    detail: serde_json::Value,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        json!({
            "stage": "evidence",
            "operator": operator,
            "field": field,
            "detail": detail,
            "writes_performed": false
        }),
        Some("Fix profile support evidence parameters and rerun canon entity evidence".to_string()),
    )
}

fn relation_hint_edge(
    candidate: &crate::entity::block::BlockCandidateRecord,
    namespace: &str,
    surfaces: &BTreeMap<&str, &PreparedSurfaceRecord>,
) -> Result<EdgeEvidenceRecord, Refusal> {
    let left_label = surfaces
        .get(candidate.left_surface_id.as_str())
        .map(|surface| surface.primary_surface.as_str())
        .unwrap_or(candidate.left_surface_id.as_str());
    let right_label = surfaces
        .get(candidate.right_surface_id.as_str())
        .map(|surface| surface.primary_surface.as_str())
        .unwrap_or(candidate.right_surface_id.as_str());
    build_edge_evidence_record(
        candidate.left_surface_id.clone(),
        candidate.right_surface_id.clone(),
        vec![EdgeEvidenceHit::new(
            ScoreLane::RelationHint,
            namespace,
            "run_candidate_review",
            "candidate_requires_review",
            ScoreUnits::from_scaled(1_000).expect("score is inside scale"),
            false,
            format!("Candidate pair retained for review: {left_label} <> {right_label}"),
        )],
    )
}

fn stage_artifacts(
    prepare: &PrepareRunArtifact,
    index: &EntityIndexRun,
    block: &BlockCandidateArtifact,
    edge: &EdgeEvidenceArtifact,
    solve: &SolveArtifact,
) -> Vec<EntityRunStageArtifact> {
    let index_ref = EntityArtifactReference {
        version: CANON_ENTITY_INDEX_VERSION_V1.to_string(),
        content_hash: index.artifact.artifact_content_hash.clone(),
    };
    let bundle_receipt_ref = EntityArtifactReference {
        version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
        content_hash: index.cache_bundle_receipt_content_hash.clone(),
    };
    vec![
        EntityRunStageArtifact {
            stage: "prepare".to_string(),
            version: CANON_ENTITY_PREPARE_VERSION_V1.to_string(),
            path: PREPARE_ARTIFACT_PATH.to_string(),
            artifact_content_hash: prepare.artifact_content_hash.clone(),
            upstream_artifacts: prepare.metadata.upstream_artifacts.clone(),
        },
        EntityRunStageArtifact {
            stage: "index".to_string(),
            version: CANON_ENTITY_INDEX_VERSION_V1.to_string(),
            path: "index/index.json".to_string(),
            artifact_content_hash: index.artifact.artifact_content_hash.clone(),
            upstream_artifacts: index.artifact.metadata.upstream_artifacts.clone(),
        },
        EntityRunStageArtifact {
            stage: cache_receipt_stage_name(index.cache_mode).to_string(),
            version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
            path: index.cache_execution_receipt_path.clone(),
            artifact_content_hash: index.cache_execution_receipt_content_hash.clone(),
            upstream_artifacts: vec![index_ref, bundle_receipt_ref],
        },
        EntityRunStageArtifact {
            stage: "block".to_string(),
            version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
            path: BLOCK_ARTIFACT_PATH.to_string(),
            artifact_content_hash: block.artifact_content_hash.clone(),
            upstream_artifacts: block.metadata.upstream_artifacts.clone(),
        },
        EntityRunStageArtifact {
            stage: "evidence".to_string(),
            version: CANON_ENTITY_EVIDENCE_VERSION_V1.to_string(),
            path: EDGE_ARTIFACT_PATH.to_string(),
            artifact_content_hash: edge.artifact_content_hash.clone(),
            upstream_artifacts: edge.metadata.upstream_artifacts.clone(),
        },
        EntityRunStageArtifact {
            stage: "solve".to_string(),
            version: CANON_ENTITY_SOLVE_VERSION_V1.to_string(),
            path: SOLVE_ARTIFACT_PATH.to_string(),
            artifact_content_hash: solve.artifact_content_hash.clone(),
            upstream_artifacts: solve.metadata.upstream_artifacts.clone(),
        },
    ]
}

fn cache_receipt_stage_name(mode: EntityIndexCacheMode) -> &'static str {
    match mode {
        EntityIndexCacheMode::Enabled => "cache_enabled",
        EntityIndexCacheMode::Disabled => "cache_disabled",
    }
}

fn load_base_strategy_reference(
    request: EntityRunRequest<'_>,
) -> Result<BaseStrategyReference, Refusal> {
    let bytes = fs::read(request.strategy).map_err(|error| {
        EntityRefusalKind::Strategy.to_refusal(
            "Failed to read entity run strategy",
            json!({
                "stage": "strategy",
                "path": request.strategy.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(next_run_command(request)),
        )
    })?;
    let content_hash = witness::hash_bytes(&bytes);
    let value = serde_yaml::from_slice::<serde_yaml::Value>(&bytes).map_err(|error| {
        EntityRefusalKind::Strategy.to_refusal(
            "Invalid entity run strategy YAML",
            json!({
                "stage": "strategy",
                "path": request.strategy.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(next_run_command(request)),
        )
    })?;
    let id = yaml_string(&value, "strategy_id")
        .or_else(|| yaml_string(&value, "profile"))
        .unwrap_or_else(|| request.profile.to_string());
    let version = yaml_string(&value, "strategy_version")
        .or_else(|| yaml_string(&value, "version"))
        .unwrap_or_else(|| "0.0.0".to_string());
    let record_link = load_record_link_runtime_config(&value, request)?;
    Ok(BaseStrategyReference {
        id,
        version,
        content_hash,
        record_link,
    })
}

fn load_record_link_runtime_config(
    value: &serde_yaml::Value,
    request: EntityRunRequest<'_>,
) -> Result<Option<RecordLinkRuntimeConfig>, Refusal> {
    let Some(section_value) = value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String("record_link".to_string())))
        .cloned()
    else {
        return Ok(None);
    };
    let section =
        serde_yaml::from_value::<RecordLinkStrategySection>(section_value).map_err(|error| {
            EntityRefusalKind::Strategy.to_refusal(
                "Invalid record-link strategy section",
                json!({
                    "stage": "strategy",
                    "field": "record_link",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some(next_run_command(request)),
            )
        })?;
    if section.inputs.len() < 2 {
        return Err(record_link_strategy_refusal(
            request,
            "inputs",
            "record-link strategy requires at least two input sidecars",
        ));
    }
    if section.feature_policies.is_empty() {
        return Err(record_link_strategy_refusal(
            request,
            "feature_policies",
            "record-link strategy requires explicit feature policies",
        ));
    }
    if section.max_candidates_per_record == 0
        || section.max_candidate_pairs == 0
        || section.max_pair_comparisons == 0
        || section.assignment_hint_score_units == 0
    {
        return Err(record_link_strategy_refusal(
            request,
            "budgets",
            "record-link budgets and assignment hint score must be non-zero",
        ));
    }
    let mut input_paths = Vec::with_capacity(section.inputs.len());
    for input in section.inputs {
        input_paths.push(validate_strategy_relative_path(
            request,
            input.path,
            "record_link.inputs.path",
        )?);
    }
    let mut feature_policies = BTreeMap::new();
    for policy in section.feature_policies {
        if feature_policies
            .insert(policy.feature_id.clone(), policy)
            .is_some()
        {
            return Err(record_link_strategy_refusal(
                request,
                "record_link.feature_policies",
                "record-link feature policies must be unique by feature_id",
            ));
        }
    }
    validate_record_link_blocking_policy(section.blocking.as_ref())
        .map_err(|error| record_link_refusal(error, "strategy", request))?;
    Ok(Some(RecordLinkRuntimeConfig {
        input_paths,
        candidate_config: RecordLinkCandidateConfig {
            operator_id: section
                .operator_id
                .unwrap_or_else(|| "record_link:exact_comparison:v1".to_string()),
            max_candidates_per_record: section.max_candidates_per_record,
            max_candidate_pairs: section.max_candidate_pairs,
            max_pair_comparisons: section.max_pair_comparisons,
            require_unique_best_per_record: section.require_unique_best_per_record.unwrap_or(true),
            feature_policies,
            blocking_policy: section.blocking,
        },
        assignment_alignment: section.assignment_alignment,
        assignment_hint_score_units: section.assignment_hint_score_units,
    }))
}

fn record_link_strategy_refusal(
    request: EntityRunRequest<'_>,
    field: &str,
    message: &str,
) -> Refusal {
    EntityRefusalKind::Strategy.to_refusal(
        message,
        json!({
            "stage": "strategy",
            "field": field,
            "writes_performed": false
        }),
        Some(next_run_command(request)),
    )
}

fn validate_strategy_relative_path(
    request: EntityRunRequest<'_>,
    path: PathBuf,
    field: &str,
) -> Result<PathBuf, Refusal> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(EntityRefusalKind::Strategy.to_refusal(
            "Record-link sidecar paths must be strategy-relative safe paths",
            json!({
                "stage": "strategy",
                "field": field,
                "path": path.display().to_string(),
                "writes_performed": false
            }),
            Some(next_run_command(request)),
        ));
    }
    Ok(path)
}

fn yaml_string(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String(key.to_string())))
        .and_then(serde_yaml::Value::as_str)
        .map(ToOwned::to_owned)
}

fn stage_strategy(base: &BaseStrategyReference, stage: &str) -> EntityStrategyReference {
    EntityStrategyReference {
        id: format!("{}.{}", base.id, stage),
        version: base.version.clone(),
        content_hash: witness::hash_bytes(format!("{}:{stage}", base.content_hash).as_bytes()),
    }
}

fn prepare_header(artifact: &PrepareRunArtifact) -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: artifact.version.clone(),
        metadata: artifact.metadata.clone(),
        summary: EntityDeterministicSummary {
            counts: artifact.summary.clone(),
            labels: BTreeMap::from([("stage".to_string(), "prepare".to_string())]),
        },
    }
}

fn read_surfaces(
    work_dir: &Path,
    prepare: &PrepareRunArtifact,
) -> Result<Vec<PreparedSurfaceRecord>, Refusal> {
    read_jsonl_file(&work_dir.join(&prepare.surfaces_path), "prepared surfaces")
}

fn posting_surfaces(surfaces: &[PreparedSurfaceRecord]) -> Vec<EntityPostingSurface> {
    surfaces
        .iter()
        .map(|surface| {
            let mut posting = EntityPostingSurface::new(surface.surface_id.clone());
            for (view_name, view) in &surface.normalized_views {
                if !view.value.trim().is_empty() {
                    posting = posting.with_exact_view(view_name.clone(), view.value.clone());
                }
            }
            posting.with_tokens(tokens_for_surface(surface))
        })
        .collect()
}

fn ngram_surfaces(surfaces: &[PreparedSurfaceRecord]) -> Vec<EntityNgramSurface> {
    surfaces
        .iter()
        .map(|surface| {
            EntityNgramSurface::new(
                surface.surface_id.clone(),
                core_view_value(&surface.profile_id, surface),
            )
        })
        .collect()
}

fn tokens_for_surface(surface: &PreparedSurfaceRecord) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    for value in surface
        .normalized_views
        .values()
        .map(|view| view.value.as_str())
        .chain(std::iter::once(surface.primary_surface.as_str()))
    {
        for token in value.split_whitespace() {
            let token = token.trim();
            if !token.is_empty() {
                tokens.insert(token.to_string());
            }
        }
    }
    tokens.into_iter().collect()
}

fn exact_bucket_surfaces(
    profile_id: &str,
    surfaces: &[PreparedSurfaceRecord],
) -> Vec<ExactBucketSurface> {
    surfaces
        .iter()
        .map(|surface| {
            ExactBucketSurface::new(
                surface.surface_id.clone(),
                core_view_value(profile_id, surface),
                surface.row_count,
                surface.deal_count,
            )
        })
        .collect()
}

fn core_view_value(profile_id: &str, surface: &PreparedSurfaceRecord) -> String {
    surface
        .normalized_views
        .get(core_view_name(profile_id))
        .or_else(|| surface.normalized_views.values().next())
        .map(|view| view.value.clone())
        .unwrap_or_else(|| surface.primary_surface.trim().to_string())
}

fn core_view_name(profile_id: &str) -> &'static str {
    match profile_id {
        "cmbs_tenant_label" => "tenant_core",
        "regab_firm_identity" => "firm_core",
        _ => "core",
    }
}

fn placeholder_bucket_values() -> BTreeSet<String> {
    [
        "0",
        "unknown",
        "vacant",
        "na",
        "n/a",
        "none",
        "placeholder:0",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

fn exact_bucket_profile(metadata: &EntityArtifactMetadata) -> ExactBucketProfile {
    ExactBucketProfile {
        id: metadata.profile.id.clone(),
        version: metadata.profile.version.clone(),
        identity_semantics: metadata.profile.identity_semantics.clone(),
        content_hash: metadata.profile.content_hash.clone().unwrap_or_default(),
    }
}

fn incumbent_ids(surfaces: &[PreparedSurfaceRecord]) -> Vec<SurfaceIncumbentId> {
    surfaces
        .iter()
        .filter_map(|surface| {
            surface
                .exact_lookup
                .canonical_id
                .as_ref()
                .map(|canonical_id| SurfaceIncumbentId {
                    surface_id: surface.surface_id.clone(),
                    canonical_id: canonical_id.clone(),
                })
        })
        .collect()
}

fn solve_provenance(surfaces: &[PreparedSurfaceRecord]) -> Vec<SolveSurfaceProvenance> {
    surfaces
        .iter()
        .map(|surface| SolveSurfaceProvenance {
            surface_id: surface.surface_id.clone(),
            row_count: surface.row_count,
            deal_count: surface.deal_count,
        })
        .collect()
}

fn solve_alias_proposal_surfaces(
    surfaces: &[PreparedSurfaceRecord],
) -> Vec<SolveAliasProposalSurface> {
    surfaces
        .iter()
        .map(|surface| SolveAliasProposalSurface {
            surface_id: surface.surface_id.clone(),
            exact_lookup_status: match surface.exact_lookup.status {
                PreparedExactLookupStatus::Resolved => SolveAliasProposalSurfaceStatus::Resolved,
                PreparedExactLookupStatus::Unresolved => {
                    SolveAliasProposalSurfaceStatus::Unresolved
                }
            },
            raw_variants: surface.raw_variants.clone(),
        })
        .collect()
}

fn read_record_link_candidate_set(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
) -> Result<Option<RecordLinkCandidateSet>, Refusal> {
    if base_strategy.record_link.is_none() {
        return Ok(None);
    }
    let bytes = match read_publication_logical_file(
        request,
        RECORD_LINK_CANDIDATES_PATH,
        "record-link candidate set",
    )? {
        Some(bytes) => bytes,
        None => {
            let path = request.work_dir.join(RECORD_LINK_CANDIDATES_PATH);
            fs::read(&path).map_err(|error| {
                EntityRefusalKind::IoBudget.to_refusal(
                    "Failed to read record-link candidate set",
                    json!({
                        "stage": "block",
                        "path": path.display().to_string(),
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                    Some(next_run_command(request)),
                )
            })?
        }
    };
    let candidate_set: RecordLinkCandidateSet = parse_json_bytes(
        &bytes,
        RECORD_LINK_CANDIDATES_PATH,
        "record-link candidate set",
    )?;
    validate_record_link_candidate_set(&candidate_set)
        .map_err(|error| record_link_refusal(error, "block", request))?;
    Ok(Some(candidate_set))
}

fn read_logical_json_file<T: DeserializeOwned>(
    request: EntityRunRequest<'_>,
    logical_path: &str,
    stable_path: &Path,
    label: &str,
) -> Result<T, Refusal> {
    match read_publication_logical_file(request, logical_path, label)? {
        Some(bytes) => parse_json_bytes(&bytes, logical_path, label),
        None => read_json_file(stable_path, label),
    }
}

fn read_logical_jsonl_file<T: DeserializeOwned>(
    request: EntityRunRequest<'_>,
    logical_path: &str,
    stable_path: &Path,
    label: &str,
) -> Result<Vec<T>, Refusal> {
    match read_publication_logical_file(request, logical_path, label)? {
        Some(bytes) => parse_jsonl_bytes(&bytes, logical_path, label),
        None => read_jsonl_file(stable_path, label),
    }
}

fn read_publication_logical_file(
    request: EntityRunRequest<'_>,
    logical_path: &str,
    label: &str,
) -> Result<Option<Vec<u8>>, Refusal> {
    read_entity_run_committed_publication_logical_bytes_inner(
        request.work_dir,
        logical_path,
        Some(label),
        Some(next_run_command(request)),
    )
}

fn read_entity_run_committed_publication_logical_bytes_inner(
    work_dir: &Path,
    logical_path: &str,
    label: Option<&str>,
    next_command: Option<String>,
) -> Result<Option<Vec<u8>>, Refusal> {
    validate_entity_run_publication_logical_path(logical_path, next_command.clone())?;
    match open_current_stream_generation(work_dir, ENTITY_RUN_PUBLICATION_STREAM_ID) {
        Ok(snapshot) => snapshot
            .read_logical_file(logical_path)
            .map(|bytes| Some(bytes.to_vec()))
            .ok_or_else(|| {
                EntityRefusalKind::ArtifactContract.to_refusal(
                    format!(
                        "Committed entity run publication is missing {}",
                        label.unwrap_or("logical file")
                    ),
                    json!({
                        "stage": "run",
                        "publication_stage": "entity_run_stage_set",
                        "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                        "generation_id": snapshot.generation_id,
                        "logical_path": logical_path,
                        "committed": true,
                        "writes_performed": false
                    }),
                    next_command.clone(),
                )
            }),
        Err(error) if is_absent_publication_stream(&error) => Ok(None),
        Err(error) => Err(publication_read_refusal(
            error,
            logical_path,
            next_command.clone(),
        )),
    }
}

fn is_absent_publication_stream(error: &EntityPublicationError) -> bool {
    error.kind == EntityPublicationErrorKind::UncommittedGeneration && error.generation_id.is_none()
}

fn validate_entity_run_publication_logical_path(
    logical_path: &str,
    next_command: Option<String>,
) -> Result<(), Refusal> {
    let path = Path::new(logical_path);
    if logical_path.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity run publication logical path is not a safe relative path",
            json!({
                "stage": "run",
                "publication_stage": "entity_run_stage_set",
                "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                "logical_path": logical_path,
                "writes_performed": false
            }),
            next_command,
        ));
    }
    Ok(())
}

fn entity_run_publication_logical_path_for_stable_path(
    work_dir: &Path,
    path: &Path,
) -> Result<&'static str, Refusal> {
    let relative_path = if path.is_absolute() {
        path.strip_prefix(work_dir).map_err(|_| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Entity run publication stable path must resolve under the work directory",
                json!({
                    "stage": "run",
                    "publication_stage": "entity_run_stage_set",
                    "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                    "work_dir": work_dir.display().to_string(),
                    "path": path.display().to_string(),
                    "writes_performed": false
                }),
                None,
            )
        })?
    } else {
        path
    };
    let logical_path = clean_relative_logical_path(relative_path)?;
    match logical_path.as_str() {
        BLOCK_ARTIFACT_PATH => Ok(BLOCK_ARTIFACT_PATH),
        BLOCK_CANDIDATES_PATH => Ok(BLOCK_CANDIDATES_PATH),
        BLOCK_DIAGNOSTICS_PATH => Ok(BLOCK_DIAGNOSTICS_PATH),
        BLOCK_EXACT_BUCKETS_PATH => Ok(BLOCK_EXACT_BUCKETS_PATH),
        RECORD_LINK_CANDIDATES_PATH => Ok(RECORD_LINK_CANDIDATES_PATH),
        EDGE_ARTIFACT_PATH => Ok(EDGE_ARTIFACT_PATH),
        EDGE_RECORDS_PATH => Ok(EDGE_RECORDS_PATH),
        RECORD_LINK_EVIDENCE_PATH => Ok(RECORD_LINK_EVIDENCE_PATH),
        ASSIGNMENT_ALIGNMENT_PATH => Ok(ASSIGNMENT_ALIGNMENT_PATH),
        SOLVE_ARTIFACT_PATH => Ok(SOLVE_ARTIFACT_PATH),
        DECISION_LEDGER_PATH => Ok(DECISION_LEDGER_PATH),
        RUN_MANIFEST_PATH => Ok(RUN_MANIFEST_PATH),
        RUN_ARTIFACT_PATH => Ok(RUN_ARTIFACT_PATH),
        LINK_MATERIALIZED_ROWS_PUBLICATION_PATH => Ok(LINK_MATERIALIZED_ROWS_PUBLICATION_PATH),
        LINK_ASSIGNMENT_ALIGNMENT_PUBLICATION_PATH => {
            Ok(LINK_ASSIGNMENT_ALIGNMENT_PUBLICATION_PATH)
        }
        LINK_OBSERVATION_SURFACE_BINDINGS_PUBLICATION_PATH => {
            Ok(LINK_OBSERVATION_SURFACE_BINDINGS_PUBLICATION_PATH)
        }
        LINK_ARTIFACT_PUBLICATION_PATH => Ok(LINK_ARTIFACT_PUBLICATION_PATH),
        _ => Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Path is not a canonical entity run publication stable path",
            json!({
                "stage": "run",
                "publication_stage": "entity_run_stage_set",
                "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                "path": path.display().to_string(),
                "logical_path": logical_path,
                "writes_performed": false
            }),
            None,
        )),
    }
}

fn clean_relative_logical_path(path: &Path) -> Result<String, Refusal> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let Some(text) = part.to_str() else {
                    return Err(EntityRefusalKind::ArtifactContract.to_refusal(
                        "Entity run publication stable path must be UTF-8",
                        json!({
                            "stage": "run",
                            "publication_stage": "entity_run_stage_set",
                            "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                            "path": path.display().to_string(),
                            "writes_performed": false
                        }),
                        None,
                    ));
                };
                parts.push(text.to_string());
            }
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(EntityRefusalKind::ArtifactContract.to_refusal(
                    "Entity run publication stable path must be a clean relative path",
                    json!({
                        "stage": "run",
                        "publication_stage": "entity_run_stage_set",
                        "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                        "path": path.display().to_string(),
                        "writes_performed": false
                    }),
                    None,
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity run publication stable path cannot be empty",
            json!({
                "stage": "run",
                "publication_stage": "entity_run_stage_set",
                "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
                "path": path.display().to_string(),
                "writes_performed": false
            }),
            None,
        ));
    }
    Ok(parts.join("/"))
}

fn publication_read_refusal(
    error: EntityPublicationError,
    logical_path: &str,
    next_command: Option<String>,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Failed to read committed entity run publication",
        json!({
            "stage": "run",
            "publication_stage": "entity_run_stage_set",
            "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
            "logical_path": logical_path,
            "error_kind": format!("{:?}", error.kind),
            "error": error.message,
            "writes_performed": false,
            "committed": error.committed,
            "generation_id": error.generation_id
        }),
        next_command,
    )
}

fn parse_json_bytes<T: DeserializeOwned>(
    bytes: &[u8],
    logical_path: &str,
    label: &str,
) -> Result<T, Refusal> {
    serde_json::from_slice(bytes).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            format!("Failed to parse {label} JSON"),
            json!({
                "stage": "run",
                "path": logical_path,
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })
}

fn parse_jsonl_bytes<T: DeserializeOwned>(
    bytes: &[u8],
    logical_path: &str,
    label: &str,
) -> Result<Vec<T>, Refusal> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            format!("Failed to decode {label} JSONL"),
            json!({
                "stage": "run",
                "path": logical_path,
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line).map_err(|error| {
                EntityRefusalKind::ArtifactContract.to_refusal(
                    format!("Failed to parse {label} JSONL"),
                    json!({
                        "stage": "run",
                        "path": logical_path,
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                    None,
                )
            })
        })
        .collect()
}

fn read_json_file<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, Refusal> {
    let text = fs::read_to_string(path).map_err(|error| {
        EntityRefusalKind::IoBudget.to_refusal(
            format!("Failed to read {label}"),
            json!({
                "stage": "run",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    serde_json::from_str(&text).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            format!("Failed to parse {label} JSON"),
            json!({
                "stage": "run",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })
}

fn read_jsonl_file<T: DeserializeOwned>(path: &Path, label: &str) -> Result<Vec<T>, Refusal> {
    let text = fs::read_to_string(path).map_err(|error| {
        EntityRefusalKind::IoBudget.to_refusal(
            format!("Failed to read {label}"),
            json!({
                "stage": "run",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line).map_err(|error| {
                EntityRefusalKind::ArtifactContract.to_refusal(
                    format!("Failed to parse {label} JSONL"),
                    json!({
                        "stage": "run",
                        "path": path.display().to_string(),
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                    None,
                )
            })
        })
        .collect()
}

fn json_publication_file<T: Serialize>(
    logical_path: &'static str,
    stage: &'static str,
    version: &'static str,
    value: &T,
) -> Result<EntityPublicationFileInput, Refusal> {
    Ok(EntityPublicationFileInput::new(
        logical_path,
        stage,
        version,
        json_bytes(logical_path, value)?,
    ))
}

fn jsonl_publication_file<T: Serialize>(
    logical_path: &'static str,
    stage: &'static str,
    version: &'static str,
    values: &[T],
) -> Result<EntityPublicationFileInput, Refusal> {
    Ok(EntityPublicationFileInput::new(
        logical_path,
        stage,
        version,
        jsonl_bytes(logical_path, values)?,
    ))
}

fn json_bytes<T: Serialize>(logical_path: &str, value: &T) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec(value).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to serialize entity artifact",
            json!({
                "stage": "run",
                "path": logical_path,
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })
}

fn jsonl_bytes<T: Serialize>(logical_path: &str, values: &[T]) -> Result<Vec<u8>, Refusal> {
    let mut bytes = Vec::new();
    for value in values {
        serde_json::to_writer(&mut bytes, value).map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Failed to serialize entity JSONL artifact",
                json!({
                    "stage": "run",
                    "path": logical_path,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                None,
            )
        })?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn mirror_publication_files(
    request: EntityRunRequest<'_>,
    publication: &EntityRunPublicationResult,
    files: &[EntityPublicationFileInput],
) -> Result<(), Refusal> {
    mirror_publication_files_at_work_dir(
        request.work_dir,
        Some(next_run_command(request)),
        publication,
        files,
    )
}

fn mirror_publication_files_at_work_dir(
    work_dir: &Path,
    next_command: Option<String>,
    publication: &EntityRunPublicationResult,
    files: &[EntityPublicationFileInput],
) -> Result<(), Refusal> {
    for file in files {
        let path = work_dir.join(&file.logical_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                publication_mirror_refusal(
                    next_command.clone(),
                    publication,
                    file,
                    &path,
                    error.to_string(),
                    false,
                )
            })?;
        }
        fs::write(&path, &file.bytes).map_err(|error| {
            publication_mirror_refusal(
                next_command.clone(),
                publication,
                file,
                &path,
                error.to_string(),
                true,
            )
        })?;
    }
    Ok(())
}

fn publication_mirror_refusal(
    next_command: Option<String>,
    publication: &EntityRunPublicationResult,
    file: &EntityPublicationFileInput,
    path: &Path,
    error: String,
    mirror_writes_performed: bool,
) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        "Failed to mirror committed entity run publication file",
        json!({
            "stage": file.stage.as_str(),
            "publication_stage": "entity_run_stage_set",
            "stream_id": publication.stream_id.as_str(),
            "generation_id": publication.generation_id.as_str(),
            "committed": publication.committed,
            "publication_outcome": publication.outcome.as_str(),
            "publication_writes_performed": publication.writes_performed,
            "mirror_writes_performed": mirror_writes_performed,
            "writes_performed": publication.writes_performed || mirror_writes_performed,
            "logical_path": file.logical_path.as_str(),
            "path": path.display().to_string(),
            "error": error,
            "post_commit_mirror": true
        }),
        next_command,
    )
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), Refusal> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to serialize entity artifact",
            json!({ "stage": "run", "path": path.display().to_string(), "error": error.to_string(), "writes_performed": false }),
            None,
        )
    })?;
    write_bytes(path, &bytes)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), Refusal> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            EntityRefusalKind::IoBudget.to_refusal(
                "Failed to create entity run artifact directory",
                json!({
                    "stage": "run",
                    "path": parent.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                None,
            )
        })?;
    }
    fs::write(path, bytes).map_err(|error| {
        EntityRefusalKind::IoBudget.to_refusal(
            "Failed to write entity run artifact",
            json!({
                "stage": "run",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })
}

fn with_run_context(mut refusal: Refusal, stage: &str, request: EntityRunRequest<'_>) -> Refusal {
    if let Some(detail) = refusal.detail.as_object_mut() {
        detail
            .entry("stage")
            .or_insert_with(|| serde_json::Value::String(stage.to_string()));
        detail.insert(
            "work_dir".to_string(),
            serde_json::Value::String(request.work_dir.display().to_string()),
        );
        detail.insert(
            "run_artifact_path".to_string(),
            serde_json::Value::String(
                request
                    .work_dir
                    .join(RUN_ARTIFACT_PATH)
                    .display()
                    .to_string(),
            ),
        );
    }
    if refusal.next_command.is_none() {
        refusal.next_command = Some(next_run_command(request));
    }
    refusal
}

fn next_run_command(request: EntityRunRequest<'_>) -> String {
    format!(
        "canon entity run {} --profile {} --strategy {} --registry {} --work-dir {}",
        request.rows.display(),
        request.profile,
        request.strategy.display(),
        request.registry.display(),
        request.work_dir.display()
    )
}

fn artifact_ref_cmp(
    left: &EntityArtifactReference,
    right: &EntityArtifactReference,
) -> std::cmp::Ordering {
    left.version
        .cmp(&right.version)
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn count(counts: &BTreeMap<String, u64>, key: &str) -> u64 {
    counts.get(key).copied().unwrap_or_default()
}

struct EntityIndexRun {
    artifact: EntityIndexArtifact,
    artifact_value: Value,
    postings: EntityPostingIndex,
    ngrams: EntityNgramIndex,
    cache_mode: EntityIndexCacheMode,
    cache_status: EntityIndexCacheStatus,
    cache_execution_receipt_path: String,
    cache_execution_receipt_content_hash: String,
    cache_bundle_receipt_path: String,
    cache_bundle_receipt_content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntityRunPublicationContext {
    cache_mode: EntityIndexCacheMode,
    cache_status: EntityIndexCacheStatus,
    cache_receipt_hash: String,
}

struct EntityBlockRun {
    artifact: BlockCandidateArtifact,
    artifact_value: Value,
    candidates: Vec<crate::entity::block::BlockCandidateRecord>,
    exact_buckets: Vec<ExactBucketAssertion>,
    record_link_candidate_set: Option<RecordLinkCandidateSet>,
    publication_context: EntityRunPublicationContext,
    publication_files: Vec<EntityPublicationFileInput>,
}

#[cfg(test)]
mod cache_runtime_tests {
    use super::*;
    use crate::entity::index_io::INDEX_ARTIFACT_FILE;
    use std::path::PathBuf;

    struct RuntimeFixture {
        rows: PathBuf,
        profile: PathBuf,
        strategy: PathBuf,
        registry: PathBuf,
    }

    impl RuntimeFixture {
        fn load() -> Self {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
                "tests/fixtures/extensions/neutral-domain/time_forward/trials/entity_disjoint/source",
            );
            Self {
                rows: root.join("reference_rows.csv"),
                profile: root.join("profile/regab_firm_identity.yaml"),
                strategy: root.join("link_strategy.yaml"),
                registry: root.join("registry"),
            }
        }

        fn request<'a>(&'a self, work_dir: &'a Path) -> EntityRunRequest<'a> {
            EntityRunRequest {
                rows: &self.rows,
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
    fn committed_publication_none_is_only_absent_stream_case() {
        let no_root = EntityPublicationError {
            kind: EntityPublicationErrorKind::UncommittedGeneration,
            message: "no root claim".to_string(),
            writes_performed: false,
            committed: Some(false),
            generation_id: None,
        };
        assert!(is_absent_publication_stream(&no_root));

        let uncommitted_root = EntityPublicationError {
            kind: EntityPublicationErrorKind::UncommittedGeneration,
            message: "root claim is not committed".to_string(),
            writes_performed: false,
            committed: Some(false),
            generation_id: Some(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            ),
        };
        assert!(!is_absent_publication_stream(&uncommitted_root));
    }

    #[test]
    fn committed_publication_stable_path_requires_canonical_stage_relative_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path();

        assert_eq!(
            entity_run_publication_logical_path_for_stable_path(
                work_dir,
                Path::new(BLOCK_ARTIFACT_PATH)
            )
            .expect("stage-relative block artifact path"),
            BLOCK_ARTIFACT_PATH
        );
        assert_eq!(
            entity_run_publication_logical_path_for_stable_path(
                work_dir,
                &work_dir.join(SOLVE_ARTIFACT_PATH)
            )
            .expect("absolute work-dir solve artifact path"),
            SOLVE_ARTIFACT_PATH
        );
        assert_eq!(
            read_entity_run_committed_publication_stable_path_bytes(
                work_dir,
                Path::new(RUN_ARTIFACT_PATH)
            )
            .expect("no stream returns None for canonical stage-relative path"),
            None
        );

        let prefixed_relative =
            PathBuf::from(work_dir.file_name().expect("tempdir name")).join(BLOCK_ARTIFACT_PATH);
        assert!(
            entity_run_publication_logical_path_for_stable_path(work_dir, &prefixed_relative)
                .is_err(),
            "relative paths prefixed with the work-dir name are not stage-relative"
        );
        assert!(
            entity_run_publication_logical_path_for_stable_path(
                work_dir,
                Path::new("./block/block.json")
            )
            .is_err()
        );
        assert!(
            entity_run_publication_logical_path_for_stable_path(
                work_dir,
                Path::new("block/../block/block.json")
            )
            .is_err()
        );
    }

    #[test]
    fn enabled_cache_mode_reports_warm_hit() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");

        let cold = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("cold enabled run rebuilds cache");
        assert_eq!(cache_label(&cold.artifact, "cache_mode"), "enabled");
        assert_eq!(cache_label(&cold.artifact, "cache_status"), "rebuilt");
        assert_eq!(
            cache_label(&cold.artifact, "cache_receipt_path"),
            RUN_CACHE_EXECUTION_RECEIPT_PATH
        );
        assert_cache_stage(&cold.artifact, "cache_enabled");
        assert_cache_receipts(
            &cold.artifact,
            &work_dir,
            EntityIndexCacheMode::Enabled,
            EntityIndexCacheStatus::Rebuilt,
            true,
        );
        assert!(work_dir.join(INDEX_CACHE_RECEIPT_FILE).is_file());
        let cold_bundle_bytes =
            std::fs::read(work_dir.join(INDEX_CACHE_RECEIPT_FILE)).expect("bundle receipt bytes");
        let cold_bundle_hash = file_hash(&work_dir.join(INDEX_CACHE_RECEIPT_FILE));

        let warm = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("warm enabled run reuses cache");
        assert_eq!(cache_label(&warm.artifact, "cache_mode"), "enabled");
        assert_eq!(cache_label(&warm.artifact, "cache_status"), "hit");
        assert_cache_stage(&warm.artifact, "cache_enabled");
        assert_cache_receipts(
            &warm.artifact,
            &work_dir,
            EntityIndexCacheMode::Enabled,
            EntityIndexCacheStatus::Hit,
            true,
        );
        assert_eq!(
            std::fs::read(work_dir.join(INDEX_CACHE_RECEIPT_FILE)).expect("bundle receipt bytes"),
            cold_bundle_bytes,
            "warm cache hit must not rewrite the immutable bundle receipt"
        );
        assert_eq!(
            file_hash(&work_dir.join(INDEX_CACHE_RECEIPT_FILE)),
            cold_bundle_hash
        );
        assert_eq!(cold.candidate_pairs, warm.candidate_pairs);

        let warm_execution_bytes = std::fs::read(work_dir.join(RUN_CACHE_EXECUTION_RECEIPT_PATH))
            .expect("warm execution receipt bytes");
        let replay = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("same-work-dir enabled replay remains a warm hit");
        assert_eq!(cache_label(&replay.artifact, "cache_status"), "hit");
        assert_cache_receipts(
            &replay.artifact,
            &work_dir,
            EntityIndexCacheMode::Enabled,
            EntityIndexCacheStatus::Hit,
            true,
        );
        assert_eq!(
            std::fs::read(work_dir.join(RUN_CACHE_EXECUTION_RECEIPT_PATH))
                .expect("replay execution receipt bytes"),
            warm_execution_bytes,
            "same-work-dir warm replay must keep stable execution receipt bytes"
        );
    }

    #[test]
    fn disabled_cache_mode_bypasses_without_poisoning_enabled_bundle() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");

        run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("enabled seed run builds reusable cache");
        let seed_bundle_bytes =
            std::fs::read(work_dir.join(INDEX_CACHE_RECEIPT_FILE)).expect("seed bundle receipt");
        let seed_bundle_hash = file_hash(&work_dir.join(INDEX_CACHE_RECEIPT_FILE));
        let disabled = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Disabled,
        )
        .expect("disabled run bypasses cache");
        assert_eq!(cache_label(&disabled.artifact, "cache_mode"), "disabled");
        assert_eq!(cache_label(&disabled.artifact, "cache_status"), "bypassed");
        assert_cache_stage(&disabled.artifact, "cache_disabled");
        assert_cache_receipts(
            &disabled.artifact,
            &work_dir,
            EntityIndexCacheMode::Disabled,
            EntityIndexCacheStatus::Bypassed,
            false,
        );
        assert_eq!(
            std::fs::read(work_dir.join(INDEX_CACHE_RECEIPT_FILE)).expect("bundle receipt bytes"),
            seed_bundle_bytes,
            "disabled bypass must not rewrite the immutable bundle receipt"
        );
        assert_eq!(
            file_hash(&work_dir.join(INDEX_CACHE_RECEIPT_FILE)),
            seed_bundle_hash
        );

        let enabled_after_disabled = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("enabled run rebuilds after disabled non-reusable receipt");
        assert_eq!(
            cache_label(&enabled_after_disabled.artifact, "cache_status"),
            "hit"
        );
        assert_cache_receipts(
            &enabled_after_disabled.artifact,
            &work_dir,
            EntityIndexCacheMode::Enabled,
            EntityIndexCacheStatus::Hit,
            true,
        );
        assert_eq!(
            std::fs::read(work_dir.join(INDEX_CACHE_RECEIPT_FILE)).expect("bundle receipt bytes"),
            seed_bundle_bytes,
            "enabled replay after disabled bypass must still leave bundle receipt immutable"
        );
    }

    #[test]
    fn disabled_index_build_bypasses_warm_hit_reader() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::TempDir::new().expect("tempdir");
        let work_dir = temp.path().join("work");

        run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("enabled seed run builds reusable cache");
        crate::entity::index::reset_v1_cache_read_probe();
        let warm = crate::entity::index::run_index_build_v1_with_cache_mode(
            crate::entity::index::EntityIndexBuildRequest {
                rows: &fixture.rows,
                profile: fixture.profile.to_str().expect("fixture profile path"),
                strategy: &fixture.strategy,
                registry: &fixture.registry,
                work_dir: &work_dir,
                max_artifact_bytes: None,
            },
            EntityIndexCacheMode::Enabled,
        )
        .expect("enabled index build consumes warm cache hit");
        assert_eq!(warm.cache_status, EntityIndexCacheStatus::Hit);
        assert_eq!(
            crate::entity::index::v1_cache_read_probe_count(),
            1,
            "enabled warm run must enter the cache-read branch"
        );

        crate::entity::index::reset_v1_cache_read_probe();
        let result = crate::entity::index::run_index_build_v1_with_cache_mode(
            crate::entity::index::EntityIndexBuildRequest {
                rows: &fixture.rows,
                profile: fixture.profile.to_str().expect("fixture profile path"),
                strategy: &fixture.strategy,
                registry: &fixture.registry,
                work_dir: &work_dir,
                max_artifact_bytes: None,
            },
            EntityIndexCacheMode::Disabled,
        )
        .expect("disabled index build bypasses warm cache hit");

        assert_eq!(result.cache_status, EntityIndexCacheStatus::Bypassed);
        assert_eq!(
            result.cache_invalidation.decision,
            crate::entity::artifact_chain::EntityCacheDecision::Miss,
            "disabled mode must rebuild instead of returning the warm-hit invalidation"
        );
        assert_eq!(
            crate::entity::index::v1_cache_read_probe_count(),
            0,
            "disabled mode must not enter the cache-read branch"
        );
    }

    #[test]
    fn enabled_cache_mode_refuses_tampered_bundle_before_hit() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");

        run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("enabled run builds cache");
        std::fs::write(index_payload_path(&work_dir), b"{\"tampered\":true}")
            .expect("tamper v1 postings");

        let refusal = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect_err("tampered cache refuses before hit");
        assert!(
            refusal.message.contains("cache receipt")
                || refusal.message.contains("cache bundle")
                || refusal.message.contains("index v1 postings")
                || refusal.detail.to_string().contains("cache_receipt"),
            "unexpected refusal: {refusal:?}"
        );
    }

    #[test]
    fn cache_receipt_refuses_unknown_fields_before_hit() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");

        run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("enabled run builds cache");
        let receipt_path = work_dir.join(INDEX_CACHE_RECEIPT_FILE);
        let mut receipt: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&receipt_path).expect("receipt bytes"))
                .expect("receipt JSON");
        receipt["unexpected"] = serde_json::json!(true);
        std::fs::write(
            &receipt_path,
            serde_json::to_vec_pretty(&receipt).expect("receipt bytes"),
        )
        .expect("tamper receipt");

        let refusal = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect_err("unknown receipt field refuses");
        assert!(
            refusal.message.contains("cache receipt")
                || refusal.detail.to_string().contains("unknown field"),
            "unexpected refusal: {refusal:?}"
        );
    }

    #[test]
    fn cache_receipt_refuses_impossible_mode_status_triples() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");

        run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect("enabled run builds cache");
        let receipt_path = work_dir.join(INDEX_CACHE_RECEIPT_FILE);
        let mut receipt: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&receipt_path).expect("receipt bytes"))
                .expect("receipt JSON");
        receipt["mode"] = serde_json::json!("disabled");
        receipt["status"] = serde_json::json!("hit");
        receipt["reusable"] = serde_json::json!(true);
        std::fs::write(
            &receipt_path,
            serde_json::to_vec_pretty(&receipt).expect("receipt bytes"),
        )
        .expect("tamper receipt");

        let refusal = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect_err("invalid receipt triple refuses");
        assert!(
            refusal.message.contains("mode/status/reusable")
                || refusal.detail.to_string().contains("disabled"),
            "unexpected refusal: {refusal:?}"
        );
    }

    #[test]
    fn enabled_and_disabled_cache_modes_preserve_semantic_outputs() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let enabled_work = temp.path().join("enabled");
        let disabled_work = temp.path().join("disabled");

        let enabled = run_entity_workbench_with_cache_mode(
            fixture.request(&enabled_work),
            EntityIndexCacheMode::Enabled,
        )
        .expect("enabled run");
        let disabled = run_entity_workbench_with_cache_mode(
            fixture.request(&disabled_work),
            EntityIndexCacheMode::Disabled,
        )
        .expect("disabled run");

        assert_eq!(enabled.candidate_pairs, disabled.candidate_pairs);
        assert_eq!(
            enabled.artifact.summary.counts,
            disabled.artifact.summary.counts
        );
        assert_eq!(
            sorted_nonempty_lines(&enabled_work.join(BLOCK_CANDIDATES_PATH)),
            sorted_nonempty_lines(&disabled_work.join(BLOCK_CANDIDATES_PATH))
        );
        assert_eq!(
            sorted_nonempty_lines(&enabled_work.join(EDGE_RECORDS_PATH)),
            sorted_nonempty_lines(&disabled_work.join(EDGE_RECORDS_PATH))
        );
        assert_eq!(
            index_domain_projection(&enabled_work, &enabled.artifact),
            index_domain_projection(&disabled_work, &disabled.artifact),
            "cache execution mode must not change the semantic index artifact fields"
        );
    }

    #[cfg(unix)]
    #[test]
    fn enabled_cache_mode_refuses_symlinked_cache_ancestor() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");
        let symlink_target = temp.path().join("outside-index");
        std::fs::create_dir_all(&work_dir).expect("work dir");
        std::fs::create_dir_all(&symlink_target).expect("symlink target");
        std::os::unix::fs::symlink(&symlink_target, work_dir.join("index"))
            .expect("symlink cache ancestor");

        let refusal = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect_err("symlinked cache ancestor refuses");
        assert!(
            refusal.message.contains("symlink") || refusal.detail.to_string().contains("symlink"),
            "unexpected refusal: {refusal:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn enabled_cache_mode_refuses_symlinked_root_index_artifact() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");
        let outside_artifact = temp.path().join("outside-index.json");
        std::fs::create_dir_all(&work_dir).expect("work dir");
        std::fs::write(&outside_artifact, b"{}").expect("outside artifact");
        std::os::unix::fs::symlink(&outside_artifact, work_dir.join(INDEX_ARTIFACT_FILE))
            .expect("index artifact symlink");

        let refusal = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect_err("symlinked root index artifact refuses before cache read");
        assert!(
            refusal.message.contains("symlink") || refusal.detail.to_string().contains("symlink"),
            "unexpected refusal: {refusal:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn enabled_cache_mode_refuses_symlinked_work_dir_before_prepare_writes() {
        let fixture = RuntimeFixture::load();
        let temp = tempfile::tempdir().expect("tempdir");
        let real_work = temp.path().join("real-work");
        let work_dir = temp.path().join("work-link");
        std::fs::create_dir_all(&real_work).expect("real work dir");
        std::os::unix::fs::symlink(&real_work, &work_dir).expect("work dir symlink");

        let refusal = run_entity_workbench_with_cache_mode(
            fixture.request(&work_dir),
            EntityIndexCacheMode::Enabled,
        )
        .expect_err("symlinked work_dir refuses before prepare writes");
        assert!(
            refusal.message.contains("symlink") || refusal.detail.to_string().contains("symlink"),
            "unexpected refusal: {refusal:?}"
        );
        assert!(
            !real_work.join("prepare").exists(),
            "prepare must not write through a symlinked work_dir"
        );
    }

    fn cache_label<'a>(artifact: &'a EntityRunArtifact, key: &str) -> &'a str {
        artifact
            .summary
            .labels
            .get(key)
            .map(String::as_str)
            .unwrap_or("")
    }

    fn assert_cache_stage(artifact: &EntityRunArtifact, expected_stage: &str) {
        let stage = artifact
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == expected_stage)
            .unwrap_or_else(|| panic!("missing {expected_stage} stage"));
        assert_eq!(stage.version, CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION);
        assert_eq!(stage.path, cache_label(artifact, "cache_receipt_path"));
        assert_eq!(
            stage.artifact_content_hash,
            cache_label(artifact, "cache_receipt_hash")
        );
        let index = artifact
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "index")
            .expect("index stage");
        assert_eq!(
            stage.upstream_artifacts,
            vec![
                EntityArtifactReference {
                    version: index.version.clone(),
                    content_hash: index.artifact_content_hash.clone(),
                },
                EntityArtifactReference {
                    version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
                    content_hash: cache_label(artifact, "cache_bundle_receipt_hash").to_string(),
                },
            ]
        );
        assert!(
            artifact
                .metadata
                .upstream_artifacts
                .iter()
                .any(|reference| {
                    reference.version == index.version
                        && reference.content_hash == index.artifact_content_hash
                })
        );
        assert!(
            artifact
                .orchestration
                .stage_order
                .iter()
                .any(|stage| { stage == expected_stage })
        );
    }

    fn assert_cache_receipts(
        artifact: &EntityRunArtifact,
        work_dir: &Path,
        mode: EntityIndexCacheMode,
        status: EntityIndexCacheStatus,
        reusable: bool,
    ) {
        let execution_path = work_dir.join(cache_label(artifact, "cache_receipt_path"));
        let execution = read_cache_receipt(&execution_path);
        assert_eq!(execution.mode, mode);
        assert_eq!(execution.status, status);
        assert_eq!(execution.reusable, reusable);
        assert_eq!(
            file_hash(&execution_path),
            cache_label(artifact, "cache_receipt_hash")
        );

        let bundle_path = work_dir.join(cache_label(artifact, "cache_bundle_receipt_path"));
        let bundle = read_cache_receipt(&bundle_path);
        assert_eq!(bundle.mode, EntityIndexCacheMode::Enabled);
        assert_eq!(bundle.status, EntityIndexCacheStatus::Rebuilt);
        assert!(bundle.reusable);
        assert_eq!(
            file_hash(&bundle_path),
            cache_label(artifact, "cache_bundle_receipt_hash")
        );
        assert_eq!(
            execution.bundle_hash, bundle.bundle_hash,
            "execution receipt must bind the immutable bundle hash"
        );
        assert_eq!(
            execution.files, bundle.files,
            "execution receipt must preserve immutable bundle file hashes"
        );
    }

    fn read_cache_receipt(path: &Path) -> EntityIndexCacheReceipt {
        serde_json::from_slice(&std::fs::read(path).expect("cache receipt bytes"))
            .expect("cache receipt JSON")
    }

    fn file_hash(path: &Path) -> String {
        witness::hash_file(path).expect("cache receipt hash")
    }

    fn index_artifact_path(artifact: &EntityRunArtifact, work_dir: &Path) -> PathBuf {
        work_dir.join(&artifact.work_dir.index_artifact_path)
    }

    fn index_payload_path(work_dir: &Path) -> PathBuf {
        let artifact: serde_json::Value =
            serde_json::from_slice(&std::fs::read(work_dir.join("index/index.json")).unwrap())
                .expect("index artifact JSON");
        work_dir.join(
            artifact["postings_path"]
                .as_str()
                .expect("index postings path"),
        )
    }

    fn index_domain_projection(work_dir: &Path, artifact: &EntityRunArtifact) -> serde_json::Value {
        let value: serde_json::Value = serde_json::from_slice(
            &std::fs::read(index_artifact_path(artifact, work_dir)).unwrap(),
        )
        .expect("index artifact JSON");
        serde_json::json!({
            "version": value["version"],
            "summary": value["summary"],
            "postings_path": value["postings_path"],
            "diagnostics_path": value["diagnostics_path"],
            "metadata": {
                "profile": value["metadata"]["profile"],
                "strategy": value["metadata"]["strategy"],
                "registry_snapshot": value["metadata"]["registry_snapshot"],
                "input": value["metadata"]["input"],
                "patch_namespace": value["metadata"]["patch_namespace"],
                "patch_set": value["metadata"]["patch_set"],
                "namekit": value["metadata"]["namekit"],
                "schema": value["metadata"]["schema"],
            }
        })
    }

    fn sorted_nonempty_lines(path: &Path) -> Vec<String> {
        let mut lines = std::fs::read_to_string(path)
            .expect("semantic artifact exists")
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                (!line.is_empty()).then(|| line.to_string())
            })
            .collect::<Vec<_>>();
        lines.sort();
        lines
    }
}
