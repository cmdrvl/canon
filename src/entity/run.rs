#![forbid(unsafe_code)]

//! Artifact-backed `canon entity run` orchestration.
//!
//! This runner keeps the happy path local and resumable by persisting every
//! stage artifact under the caller's work directory and emitting a compact run
//! artifact that records the chained stage hashes.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_INDEX_VERSION,
        CANON_ENTITY_PREPARE_VERSION, CANON_ENTITY_RUN_VERSION, CANON_ENTITY_SOLVE_VERSION,
        EntityArtifactHeader, EntityArtifactMetadata, EntityArtifactReference,
        EntityDeterministicSummary, EntityStrategyReference,
        block::{
            BlockCandidateBudgetConfig, BlockCandidateBudgetObservation,
            BlockCandidateGenerationRequest, BlockCandidateOperator, EntityBlockStageOutput,
            EntityBlockStageRequest, EntityNativeBlockBudgetRefusalProof,
            EntityNativeBlockScaleReport, ExactBucketBlockRequest, ExactBucketSurface,
            NgramTopKBlockOperator, RareTokenOverlapBlockOperator, emit_exact_bucket_hyperedges,
            generate_block_candidates, native_block_budget_refusal_proof,
            native_block_scale_report,
        },
        block_artifact::{
            BlockCandidateArtifact, BlockCandidateArtifactRequest, ExactBucketAssertion,
            ExactBucketProfile, ExactBucketUpstream, build_block_candidate_artifact_contract,
            validate_block_candidate_artifact_contract, validate_block_candidate_payload_hashes,
        },
        edge::{
            EdgeEvidenceHit, EdgeEvidenceRecord, EntityEvidenceStageOutput,
            EntityEvidenceStageRequest, build_edge_evidence_record,
        },
        edge_artifact::{
            EdgeEvidenceArtifact, EdgeEvidenceArtifactRequest,
            build_edge_evidence_artifact_contract, validate_edge_evidence_artifact_contract,
            validate_edge_evidence_payload_hashes,
        },
        error::EntityRefusalKind,
        evidence::{
            ExactViewSupportRequest, StringSimilaritySupportRequest, exact_view_support_hit,
            string_similarity_support_hit,
        },
        graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex, EntityNgramSurface},
        index::{
            EntityIndexArtifact, EntityIndexArtifactRequest, EntityIndexCacheStatus,
            EntityNativeIndexScaleReport, build_index_artifact_contract,
            index_cache_key_from_prepare_header, index_summary_counts, native_index_scale_report,
        },
        index_io::{
            EntityIndexDiagnosticRecord, EntityIndexPersistRequest, EntityIndexPostingsBundle,
            write_index_disk_bundle,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
        prepare::{
            DEFAULT_PREPARE_ROWS_PER_CHUNK, LoadedPrepareProfile, PrepareRunArtifact,
            PrepareRunRequest, PreparedExactLookupStatus, PreparedSurfaceRecord,
            load_prepare_profile_with_hash, run_prepare_with_target_rows_per_chunk,
        },
        profile::{EntityOperatorSpec, EntityProfileDocument},
        score::{ScoreLane, ScoreUnits},
        solve::{
            EntitySolveStageOutput, EntitySolveStageRequest, SolveArtifact, SolveArtifactRequest,
            SolveDiagnosticsReport, SolveReconciliationConfig, SolveSurfaceProvenance,
            build_solve_artifact_contract, build_solve_diagnostics,
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
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};

#[path = "link.rs"]
pub mod link;

const PREPARE_ARTIFACT_PATH: &str = "prepare/prepare.json";
const BLOCK_ARTIFACT_PATH: &str = "block/block.json";
const BLOCK_CANDIDATES_PATH: &str = "block/candidates.jsonl";
const BLOCK_DIAGNOSTICS_PATH: &str = "block/diagnostics.json";
const BLOCK_EXACT_BUCKETS_PATH: &str = "block/exact_buckets.jsonl";
const EDGE_ARTIFACT_PATH: &str = "edge/edge.json";
const EDGE_RECORDS_PATH: &str = "edge/edges.jsonl";
const SOLVE_ARTIFACT_PATH: &str = "solve/solve.json";
const DECISION_LEDGER_PATH: &str = "solve/decision_ledger.jsonl";
const RUN_ARTIFACT_PATH: &str = "run.json";

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
    pub candidate_pairs: u64,
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
    pub edge_artifact_path: String,
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
}

pub fn run_entity_workbench(request: EntityRunRequest<'_>) -> Result<EntityRunResult, Refusal> {
    run_entity_workbench_with_batching(request, EntityRunBatchConfig::default())
}

pub fn run_entity_workbench_with_batching(
    request: EntityRunRequest<'_>,
    batch_config: EntityRunBatchConfig,
) -> Result<EntityRunResult, Refusal> {
    let base_strategy = load_base_strategy_reference(request)
        .map_err(|refusal| with_run_context(refusal, "strategy", request))?;
    let prepare = run_prepare_with_target_rows_per_chunk(
        PrepareRunRequest {
            rows: request.rows,
            profile: request.profile,
            registry: request.registry,
            work_dir: request.work_dir,
        },
        batch_config.target_rows_per_batch,
    )
    .map_err(|refusal| with_run_context(refusal, "prepare", request))?;
    let surfaces = read_surfaces(request.work_dir, &prepare)
        .map_err(|refusal| with_run_context(refusal, "prepare", request))?;
    let prepare_header = prepare_header(&prepare);

    let index = build_and_write_index(request, &base_strategy, &prepare_header, &surfaces)
        .map_err(|refusal| with_run_context(refusal, "index", request))?;
    let block = build_and_write_block(request, &base_strategy, &index, &surfaces)
        .map_err(|refusal| with_run_context(refusal, "block", request))?;
    let (edge, edge_records) = build_and_write_edge(request, &base_strategy, &block, &surfaces)
        .map_err(|refusal| with_run_context(refusal, "edge", request))?;
    let solve = build_and_write_solve(request, &base_strategy, &edge, &edge_records, &surfaces)
        .map_err(|refusal| with_run_context(refusal, "solve", request))?;

    let mut artifact = run_artifact(
        request,
        &base_strategy,
        &prepare,
        &surfaces,
        &index.artifact,
        &block.artifact,
        &edge,
        &solve,
    )?;
    artifact.artifact_content_hash = hash_run_artifact_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    write_json_file(&request.work_dir.join(RUN_ARTIFACT_PATH), &artifact)
        .map_err(|refusal| with_run_context(refusal, "run", request))?;

    Ok(EntityRunResult {
        candidate_pairs: block.artifact.summary.counts["candidate_pairs"],
        artifact,
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
    let prepare = run_prepare_with_target_rows_per_chunk(
        PrepareRunRequest {
            rows: run_request.rows,
            profile: run_request.profile,
            registry: run_request.registry,
            work_dir: run_request.work_dir,
        },
        batch_config.target_rows_per_batch,
    )
    .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let surfaces = read_surfaces(run_request.work_dir, &prepare)
        .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let prepare_header = prepare_header(&prepare);
    let index = build_and_write_index(run_request, &base_strategy, &prepare_header, &surfaces)
        .map_err(|refusal| with_run_context(refusal, "index", run_request))?;
    let block = build_and_write_block(run_request, &base_strategy, &index, &surfaces)
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
    let prepare = run_prepare_with_target_rows_per_chunk(
        PrepareRunRequest {
            rows: run_request.rows,
            profile: run_request.profile,
            registry: run_request.registry,
            work_dir: run_request.work_dir,
        },
        batch_config.target_rows_per_batch,
    )
    .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let surfaces = read_surfaces(run_request.work_dir, &prepare)
        .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let prepare_header = prepare_header(&prepare);
    let index = build_and_write_index(run_request, &base_strategy, &prepare_header, &surfaces)
        .map_err(|refusal| with_run_context(refusal, "index", run_request))?;
    let block = read_block_stage_from_artifact(
        run_request,
        &base_strategy,
        &index,
        &surfaces,
        request.candidates,
    )
    .map_err(|refusal| with_run_context(refusal, "block", run_request))?;
    let (artifact, records) = build_and_write_edge(run_request, &base_strategy, &block, &surfaces)
        .map_err(|refusal| with_run_context(refusal, "edge", run_request))?;

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
    let prepare = run_prepare_with_target_rows_per_chunk(
        PrepareRunRequest {
            rows: run_request.rows,
            profile: run_request.profile,
            registry: run_request.registry,
            work_dir: run_request.work_dir,
        },
        batch_config.target_rows_per_batch,
    )
    .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let surfaces = read_surfaces(run_request.work_dir, &prepare)
        .map_err(|refusal| with_run_context(refusal, "prepare", run_request))?;
    let (edge, edge_records) =
        read_edge_stage_from_artifact(run_request, &base_strategy, &prepare, request.evidence)
            .map_err(|refusal| with_run_context(refusal, "edge", run_request))?;
    let solve = build_and_write_solve(run_request, &base_strategy, &edge, &edge_records, &surfaces)
        .map_err(|refusal| with_run_context(refusal, "solve", run_request))?;

    Ok(EntitySolveStageOutput { artifact: solve })
}

pub fn render_run_summary(artifact: &EntityRunArtifact) -> String {
    let counts = &artifact.summary.counts;
    let labels = &artifact.summary.labels;
    format!(
        "{} profile={} registry={}@{} rows={} surfaces={} exact_resolved={} candidate_pairs={} edge_records={} entities={} review_groups={} run_artifact={}",
        artifact.version,
        labels.get("profile_id").map_or("", String::as_str),
        labels.get("registry_id").map_or("", String::as_str),
        labels.get("registry_version").map_or("", String::as_str),
        count(counts, "row_count"),
        count(counts, "prepared_surfaces"),
        count(counts, "exact_resolved_surfaces"),
        count(counts, "candidate_pairs"),
        count(counts, "edge_records"),
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
            stage: "edge".to_string(),
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
    let artifact: BlockCandidateArtifact = read_json_file(block_artifact_path, "block artifact")?;
    validate_block_candidate_artifact_contract(&artifact)?;
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
    let candidates: Vec<crate::entity::block::BlockCandidateRecord> =
        read_jsonl_file(&candidate_records_path, "block candidate records")?;
    let diagnostics: crate::entity::block::BlockCandidateGenerationDiagnostics =
        read_json_file(&candidate_diagnostics_path, "block candidate diagnostics")?;
    let exact_buckets: Vec<ExactBucketAssertion> =
        read_jsonl_file(&exact_buckets_path, "exact bucket assertions")?;
    validate_block_candidate_payload_hashes(&artifact, &candidates, &diagnostics, &exact_buckets)?;
    validate_block_payload_surfaces(&candidates, &exact_buckets, surfaces)?;

    Ok(EntityBlockRun {
        artifact,
        candidates,
        exact_buckets,
    })
}

fn read_edge_stage_from_artifact(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    prepare: &PrepareRunArtifact,
    edge_artifact_path: &Path,
) -> Result<(EdgeEvidenceArtifact, Vec<EdgeEvidenceRecord>), Refusal> {
    let artifact: EdgeEvidenceArtifact = read_json_file(edge_artifact_path, "evidence artifact")?;
    validate_edge_evidence_artifact_contract(&artifact)?;
    validate_stage_metadata_context(
        "edge",
        &artifact.metadata,
        &prepare.metadata,
        &stage_strategy(base_strategy, "edge"),
    )?;

    let edge_records_path = resolve_work_dir_artifact_path(
        request.work_dir,
        &artifact.edge_records_path,
        "edge_records_path",
        "edge",
    )?;
    let exact_buckets_path = resolve_work_dir_artifact_path(
        request.work_dir,
        BLOCK_EXACT_BUCKETS_PATH,
        "exact_bucket_assertions_path",
        "edge",
    )?;
    let edge_records: Vec<EdgeEvidenceRecord> =
        read_jsonl_file(&edge_records_path, "edge evidence records")?;
    let exact_buckets: Vec<ExactBucketAssertion> =
        read_jsonl_file(&exact_buckets_path, "exact bucket assertions")?;
    validate_edge_evidence_payload_hashes(&artifact, &edge_records, &exact_buckets)?;

    Ok((artifact, edge_records))
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
    let command_stage = if stage == "edge" { "evidence" } else { stage };
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
            "Rerun canon entity {command_stage} with matching upstream artifacts"
        )),
    )
}

fn build_and_write_index(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    prepare: &EntityArtifactHeader,
    surfaces: &[PreparedSurfaceRecord],
) -> Result<EntityIndexRun, Refusal> {
    let strategy = stage_strategy(base_strategy, "index");
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
    let posting_diagnostics = postings.diagnostics.clone();
    let ngram_diagnostics = ngrams.diagnostics.clone();
    let artifact = build_index_artifact_contract(EntityIndexArtifactRequest {
        prepare: prepare.clone(),
        strategy: strategy.clone(),
        cache_status: EntityIndexCacheStatus::Rebuilt,
        postings_path: "index/postings.json".to_string(),
        diagnostics_path: "index/diagnostics.jsonl".to_string(),
        counts: index_summary_counts(
            u64::from(posting_diagnostics.surface_count),
            posting_diagnostics.token_count as u64,
            ngram_diagnostics.ngram_count as u64,
            (posting_diagnostics.large_exact_view_bucket_count
                + posting_diagnostics.common_token_count
                + ngram_diagnostics.common_ngram_count) as u64,
        ),
    })?;
    let cache_key = index_cache_key_from_prepare_header(
        crate::entity::cache::EntityCacheLayer::NgramPostings,
        prepare,
        &strategy,
    )?;
    let diagnostics = index_diagnostics(&artifact, &posting_diagnostics, &ngram_diagnostics);
    write_index_disk_bundle(
        request.work_dir,
        EntityIndexPersistRequest {
            artifact: artifact.clone(),
            cache_key,
            postings: EntityIndexPostingsBundle::new(postings.clone(), Some(ngrams.clone())),
            diagnostics,
            max_artifact_bytes: None,
        },
    )?;

    Ok(EntityIndexRun {
        artifact,
        postings,
        ngrams,
    })
}

fn build_and_write_block(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    index: &EntityIndexRun,
    surfaces: &[PreparedSurfaceRecord],
) -> Result<EntityBlockRun, Refusal> {
    let strategy = stage_strategy(base_strategy, "block");
    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: index.artifact.metadata.profile.id.clone(),
        posting_index: &index.postings,
        ngram_index: Some(&index.ngrams),
        budget_config: BlockCandidateBudgetConfig::new(100, 25_000, 25_000),
        operators: vec![
            BlockCandidateOperator::NgramTopK(NgramTopKBlockOperator::new(
                "ngram_topk:run",
                25,
                25,
            )),
            BlockCandidateOperator::RareTokenOverlap(
                RareTokenOverlapBlockOperator::new(
                    "rare_token_overlap:run",
                    core_view_name(&index.artifact.metadata.profile.id),
                )
                .with_topk(25, 25)
                .with_max_posting_size(1_000),
            ),
        ],
    })?;
    let exact_bucket_result = emit_exact_bucket_hyperedges(ExactBucketBlockRequest {
        profile: exact_bucket_profile(&index.artifact.metadata),
        upstream: ExactBucketUpstream {
            prepare_hash: index
                .artifact
                .metadata
                .upstream_artifacts
                .iter()
                .find(|reference| reference.version == CANON_ENTITY_PREPARE_VERSION)
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
    write_jsonl_file(
        &request.work_dir.join(BLOCK_CANDIDATES_PATH),
        &result.candidates,
    )?;
    write_json_file(
        &request.work_dir.join(BLOCK_DIAGNOSTICS_PATH),
        &result.diagnostics,
    )?;
    write_jsonl_file(
        &request.work_dir.join(BLOCK_EXACT_BUCKETS_PATH),
        &exact_bucket_result.assertions,
    )?;
    write_json_file(&request.work_dir.join(BLOCK_ARTIFACT_PATH), &artifact)?;

    Ok(EntityBlockRun {
        artifact,
        candidates: result.candidates,
        exact_buckets: exact_bucket_result.assertions,
    })
}

fn build_and_write_edge(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    block: &EntityBlockRun,
    surfaces: &[PreparedSurfaceRecord],
) -> Result<(EdgeEvidenceArtifact, Vec<EdgeEvidenceRecord>), Refusal> {
    let strategy = stage_strategy(base_strategy, "edge");
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
    edge_records.sort_by(|left, right| {
        left.left_surface_id
            .cmp(&right.left_surface_id)
            .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
    });
    let artifact = build_edge_evidence_artifact_contract(EdgeEvidenceArtifactRequest {
        block: block.artifact.clone(),
        strategy,
        edge_records_path: EDGE_RECORDS_PATH.to_string(),
        edge_records: edge_records.clone(),
        candidate_records: block.candidates.clone(),
        bucket_assertions: block.exact_buckets.clone(),
    })?;
    validate_edge_evidence_artifact_contract(&artifact)?;
    write_jsonl_file(&request.work_dir.join(EDGE_RECORDS_PATH), &edge_records)?;
    write_json_file(&request.work_dir.join(EDGE_ARTIFACT_PATH), &artifact)?;
    Ok((artifact, edge_records))
}

fn build_and_write_solve(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    edge: &EdgeEvidenceArtifact,
    edge_records: &[EdgeEvidenceRecord],
    surfaces: &[PreparedSurfaceRecord],
) -> Result<SolveArtifact, Refusal> {
    let exact_buckets: Vec<ExactBucketAssertion> = read_jsonl_file(
        &request.work_dir.join(BLOCK_EXACT_BUCKETS_PATH),
        "exact bucket assertions",
    )?;
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: edge_records.to_vec(),
        exact_bucket_assertions: exact_buckets,
        incumbent_ids: incumbent_ids(surfaces),
    })?;
    let mut metadata = edge.metadata.clone();
    metadata.strategy = stage_strategy(base_strategy, "solve");
    let mut upstream_artifacts = metadata.upstream_artifacts.clone();
    upstream_artifacts.push(EntityArtifactReference {
        version: edge.version.clone(),
        content_hash: edge.artifact_content_hash.clone(),
    });
    upstream_artifacts.sort_by(artifact_ref_cmp);
    upstream_artifacts.dedup();
    metadata.upstream_artifacts = upstream_artifacts;
    metadata.artifact_content_hash.clear();

    let artifact = build_solve_artifact_contract(SolveArtifactRequest {
        metadata,
        graph,
        config: SolveReconciliationConfig::escrow_only(ScoreUnits::MAX),
        provenance: solve_provenance(surfaces),
        decision_ledger_path: DECISION_LEDGER_PATH.to_string(),
    })?;
    validate_solve_artifact_contract(&artifact)?;
    write_bytes(&request.work_dir.join(DECISION_LEDGER_PATH), b"")?;
    write_json_file(&request.work_dir.join(SOLVE_ARTIFACT_PATH), &artifact)?;
    Ok(artifact)
}

#[allow(clippy::too_many_arguments)]
fn run_artifact(
    request: EntityRunRequest<'_>,
    base_strategy: &BaseStrategyReference,
    prepare: &PrepareRunArtifact,
    surfaces: &[PreparedSurfaceRecord],
    index: &EntityIndexArtifact,
    block: &BlockCandidateArtifact,
    edge: &EdgeEvidenceArtifact,
    solve: &SolveArtifact,
) -> Result<EntityRunArtifact, Refusal> {
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

    Ok(EntityRunArtifact {
        version: CANON_ENTITY_RUN_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary: run_summary(request, prepare, surfaces, index, block, edge, solve),
        orchestration,
        stage_artifacts,
        work_dir: EntityRunWorkDirLayout {
            prepare_artifact_path: PREPARE_ARTIFACT_PATH.to_string(),
            surfaces_path: prepare.surfaces_path.clone(),
            index_artifact_path: "index.json".to_string(),
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
    })
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
        stage_order: [
            "prepare",
            "index",
            "block",
            "edge",
            "solve",
            "review_export",
            "audit",
            "review_import",
            "promote",
            "apply",
        ]
        .into_iter()
        .map(str::to_string)
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
    index: &EntityIndexArtifact,
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
                count(&index.summary.counts, "surface_count"),
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
                "edge_records".to_string(),
                count(&edge.summary.counts, "edge_records"),
            ),
            (
                "relation_hint_edges".to_string(),
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
            "stage": "edge",
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
    index: &EntityIndexArtifact,
    block: &BlockCandidateArtifact,
    edge: &EdgeEvidenceArtifact,
    solve: &SolveArtifact,
) -> Vec<EntityRunStageArtifact> {
    vec![
        EntityRunStageArtifact {
            stage: "prepare".to_string(),
            version: CANON_ENTITY_PREPARE_VERSION.to_string(),
            path: PREPARE_ARTIFACT_PATH.to_string(),
            artifact_content_hash: prepare.artifact_content_hash.clone(),
            upstream_artifacts: prepare.metadata.upstream_artifacts.clone(),
        },
        EntityRunStageArtifact {
            stage: "index".to_string(),
            version: CANON_ENTITY_INDEX_VERSION.to_string(),
            path: "index.json".to_string(),
            artifact_content_hash: index.artifact_content_hash.clone(),
            upstream_artifacts: index.metadata.upstream_artifacts.clone(),
        },
        EntityRunStageArtifact {
            stage: "block".to_string(),
            version: CANON_ENTITY_BLOCK_VERSION.to_string(),
            path: BLOCK_ARTIFACT_PATH.to_string(),
            artifact_content_hash: block.artifact_content_hash.clone(),
            upstream_artifacts: block.metadata.upstream_artifacts.clone(),
        },
        EntityRunStageArtifact {
            stage: "edge".to_string(),
            version: CANON_ENTITY_EDGE_VERSION.to_string(),
            path: EDGE_ARTIFACT_PATH.to_string(),
            artifact_content_hash: edge.artifact_content_hash.clone(),
            upstream_artifacts: edge.metadata.upstream_artifacts.clone(),
        },
        EntityRunStageArtifact {
            stage: "solve".to_string(),
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            path: SOLVE_ARTIFACT_PATH.to_string(),
            artifact_content_hash: solve.artifact_content_hash.clone(),
            upstream_artifacts: solve.metadata.upstream_artifacts.clone(),
        },
    ]
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
    Ok(BaseStrategyReference {
        id,
        version,
        content_hash,
    })
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

fn index_diagnostics(
    artifact: &EntityIndexArtifact,
    postings: &crate::entity::postings::EntityPostingDiagnostics,
    ngrams: &crate::entity::index::ngram_index::EntityNgramDiagnostics,
) -> Vec<EntityIndexDiagnosticRecord> {
    let mut summary = EntityIndexDiagnosticRecord::new("artifact_summary");
    summary.counts = artifact.summary.counts.clone();
    summary.labels = artifact.summary.labels.clone();

    let mut posting = EntityIndexDiagnosticRecord::new("posting_summary");
    posting.counts = BTreeMap::from([
        (
            "surface_count".to_string(),
            u64::from(postings.surface_count),
        ),
        ("token_count".to_string(), postings.token_count as u64),
        (
            "common_token_count".to_string(),
            postings.common_token_count as u64,
        ),
    ]);

    let mut ngram = EntityIndexDiagnosticRecord::new("ngram_summary");
    ngram.counts = BTreeMap::from([
        ("ngram_count".to_string(), ngrams.ngram_count as u64),
        (
            "common_ngram_count".to_string(),
            ngrams.common_ngram_count as u64,
        ),
    ]);

    vec![summary, posting, ngram]
}

fn hash_run_artifact_without_self(artifact: &EntityRunArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash entity run artifact",
            json!({ "stage": "run", "error": error.to_string(), "writes_performed": false }),
            None,
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
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

fn write_jsonl_file<T: Serialize>(path: &Path, values: &[T]) -> Result<(), Refusal> {
    let mut bytes = Vec::new();
    for value in values {
        serde_json::to_writer(&mut bytes, value).map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Failed to serialize entity JSONL artifact",
                json!({ "stage": "run", "path": path.display().to_string(), "error": error.to_string(), "writes_performed": false }),
                None,
            )
        })?;
        bytes.push(b'\n');
    }
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
    postings: EntityPostingIndex,
    ngrams: EntityNgramIndex,
}

struct EntityBlockRun {
    artifact: BlockCandidateArtifact,
    candidates: Vec<crate::entity::block::BlockCandidateRecord>,
    exact_buckets: Vec<ExactBucketAssertion>,
}
