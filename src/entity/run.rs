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
            BlockCandidateBudgetConfig, BlockCandidateGenerationRequest, BlockCandidateOperator,
            ExactBucketBlockRequest, ExactBucketSurface, NgramTopKBlockOperator,
            RareTokenOverlapBlockOperator, emit_exact_bucket_hyperedges, generate_block_candidates,
        },
        block_artifact::{
            BlockCandidateArtifact, BlockCandidateArtifactRequest, ExactBucketAssertion,
            ExactBucketProfile, ExactBucketUpstream, build_block_candidate_artifact_contract,
            validate_block_candidate_artifact_contract,
        },
        edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
        edge_artifact::{
            EdgeEvidenceArtifact, EdgeEvidenceArtifactRequest,
            build_edge_evidence_artifact_contract, validate_edge_evidence_artifact_contract,
        },
        error::EntityRefusalKind,
        graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex, EntityNgramSurface},
        index::{
            EntityIndexArtifact, EntityIndexArtifactRequest, EntityIndexCacheStatus,
            build_index_artifact_contract, index_cache_key_from_prepare_header,
            index_summary_counts,
        },
        index_io::{
            EntityIndexDiagnosticRecord, EntityIndexPersistRequest, EntityIndexPostingsBundle,
            write_index_disk_bundle,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
        prepare::{
            DEFAULT_PREPARE_ROWS_PER_CHUNK, PrepareRunArtifact, PrepareRunRequest,
            PreparedExactLookupStatus, PreparedSurfaceRecord,
            run_prepare_with_target_rows_per_chunk,
        },
        score::{ScoreLane, ScoreUnits},
        solve::{
            SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig, SolveSurfaceProvenance,
            build_solve_artifact_contract, validate_solve_artifact_contract,
        },
    },
    namekit::ngram::NgramConfig,
    witness,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const PREPARE_ARTIFACT_PATH: &str = "prepare/prepare.json";
const BLOCK_ARTIFACT_PATH: &str = "block/block.json";
const BLOCK_CANDIDATES_PATH: &str = "block/candidates.jsonl";
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
        budget_config: BlockCandidateBudgetConfig::new(100, 1_000, 25_000),
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
        candidate_records: result.candidates.clone(),
        bucket_assertions: exact_bucket_result.assertions.clone(),
        known_surface_ids: surfaces
            .iter()
            .map(|surface| surface.surface_id.clone())
            .collect(),
        diagnostics: result.diagnostics,
    })?;
    validate_block_candidate_artifact_contract(&artifact)?;
    write_jsonl_file(
        &request.work_dir.join(BLOCK_CANDIDATES_PATH),
        &result.candidates,
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
    let namespace = block
        .artifact
        .metadata
        .profile
        .patch_namespaces
        .relations
        .clone();
    let surface_lookup = surfaces
        .iter()
        .map(|surface| (surface.surface_id.as_str(), surface))
        .collect::<BTreeMap<_, _>>();
    let mut edge_records = block
        .candidates
        .iter()
        .map(|candidate| relation_hint_edge(candidate, &namespace, &surface_lookup))
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

    Ok(EntityRunArtifact {
        version: CANON_ENTITY_RUN_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary: run_summary(request, prepare, surfaces, index, block, edge, solve),
        stage_artifacts,
        work_dir: EntityRunWorkDirLayout {
            prepare_artifact_path: PREPARE_ARTIFACT_PATH.to_string(),
            surfaces_path: prepare.surfaces_path.clone(),
            index_artifact_path: "index.json".to_string(),
            block_artifact_path: BLOCK_ARTIFACT_PATH.to_string(),
            candidate_records_path: BLOCK_CANDIDATES_PATH.to_string(),
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
