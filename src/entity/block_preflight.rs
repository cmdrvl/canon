#![forbid(unsafe_code)]

//! Read-only candidate cardinality and skew preflight for `canon entity block`.

use crate::{
    Refusal,
    entity::{
        EntityProfileReference, EntityStrategyReference,
        block::{
            BlockCandidateBudgetConfig, BlockCandidateGenerationDiagnostics,
            BlockCandidateGenerationRequest, BlockCandidateOperator, BlockCandidateRecord,
            BlockOperatorCandidateDiagnostics, BlockRuntimeConfig,
            default_block_candidate_operators, generate_block_candidates,
            load_block_runtime_config,
        },
        error::EntityRefusalKind,
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex},
        index::{
            self as entity_index, DEFAULT_INDEX_COMMON_POSTING_LIMIT, DEFAULT_INDEX_NGRAM_WIDTH,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, PostingFeatureKind},
        prepare::{
            LoadedPrepareProfile, PrepareInputContract, PreparedAnchor, PreparedInputObservation,
            PreparedSurface, PreparedSurfaceRecord, load_prepare_profile_with_hash,
            prepare_contract_for_loaded_profile, prepare_surface_records_for_loaded_profile,
            project_prepare_path,
        },
    },
    namekit::ngram::NgramConfig,
    witness,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const CANON_ENTITY_BLOCK_PREFLIGHT_VERSION: &str = "canon_entity_block_preflight.v1";
const PREFLIGHT_STAGE: &str = "block_preflight";
const DEFAULT_TOP_BLOCKS: usize = 10;
const SAMPLE_MODULUS: u64 = 10_000;

#[derive(Debug, Clone, Copy)]
pub struct EntityBlockPreflightRequest<'a> {
    pub rows: &'a Path,
    pub profile: &'a str,
    pub strategy: &'a Path,
    pub sample_pct: u8,
    pub work_dir: Option<&'a Path>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityBlockPreflightReport {
    pub version: String,
    pub rows: BlockPreflightInputReference,
    pub profile: EntityProfileReference,
    pub strategy: EntityStrategyReference,
    pub sample: BlockPreflightSampleReport,
    pub configured_budgets: BlockPreflightBudgetConfigReport,
    pub budget_verdict: BlockPreflightBudgetVerdict,
    pub totals: BlockPreflightTotals,
    pub operators: Vec<BlockPreflightOperatorReport>,
    pub top_blocks: Vec<BlockPreflightTopBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockPreflightInputReference {
    pub source: String,
    pub content_hash: String,
    pub row_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockPreflightSampleReport {
    pub requested_pct: u8,
    pub exact: bool,
    pub hash_modulus: u64,
    pub hash_threshold: u64,
    pub input_row_count: u64,
    pub sampled_row_count: u64,
    pub surface_count: u64,
    pub sampled_surface_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockPreflightBudgetConfigReport {
    pub max_candidates_per_surface: u64,
    pub max_candidates_per_operator: u64,
    pub max_candidates_per_run: u64,
    pub max_exact_bucket_size: u64,
}

impl From<&BlockRuntimeConfig> for BlockPreflightBudgetConfigReport {
    fn from(value: &BlockRuntimeConfig) -> Self {
        Self {
            max_candidates_per_surface: value.candidate_budget.max_candidates_per_surface,
            max_candidates_per_operator: value.candidate_budget.max_candidates_per_operator,
            max_candidates_per_run: value.candidate_budget.max_candidates_per_run,
            max_exact_bucket_size: value.max_exact_bucket_size,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockPreflightBudgetStatus {
    Pass,
    Tight,
    WouldRefuse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockPreflightBudgetVerdict {
    pub status: BlockPreflightBudgetStatus,
    pub checks: Vec<BlockPreflightBudgetCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockPreflightBudgetCheck {
    pub policy_id: String,
    pub status: BlockPreflightBudgetStatus,
    pub observed: u64,
    pub estimated: u64,
    pub configured: u64,
    pub subject_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockPreflightTotals {
    pub observed_candidate_record_count: u64,
    pub estimated_candidate_record_count: u64,
    pub observed_candidate_pairs_emitted: u64,
    pub estimated_candidate_pairs_emitted: u64,
    pub observed_candidate_pairs_suppressed_by_cap: u64,
    pub estimated_candidate_pairs_suppressed_by_cap: u64,
    pub observed_suppressed_candidate_count: u64,
    pub estimated_suppressed_candidate_count: u64,
    pub observed_large_buckets_suppressed: u64,
    pub estimated_large_buckets_suppressed: u64,
    pub observed_max_candidates_for_surface: u64,
    pub estimated_max_candidates_for_surface: u64,
    pub observed_max_candidates_for_operator: u64,
    pub estimated_max_candidates_for_operator: u64,
    pub candidate_pairs_per_surface_p50: u64,
    pub candidate_pairs_per_surface_p95: u64,
    pub candidate_pairs_per_surface_p99: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockPreflightOperatorReport {
    pub operator_id: String,
    pub observed_input_candidate_count: u64,
    pub estimated_input_candidate_count: u64,
    pub observed_eligible_candidate_count: u64,
    pub estimated_eligible_candidate_count: u64,
    pub observed_emitted_candidate_count: u64,
    pub estimated_emitted_candidate_count: u64,
    pub observed_suppressed_candidate_count: u64,
    pub estimated_suppressed_candidate_count: u64,
    pub observed_large_posting_suppressed_count: u64,
    pub estimated_large_posting_suppressed_count: u64,
    pub observed_marginal_candidate_count: u64,
    pub estimated_marginal_candidate_count: u64,
    pub observed_cumulative_candidate_count: u64,
    pub estimated_cumulative_candidate_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockPreflightTopBlock {
    pub operator_id: String,
    pub key_kind: String,
    pub key_value: String,
    pub observed_surface_count: u64,
    pub estimated_surface_count: u64,
    pub observed_row_count: u64,
    pub estimated_row_count: u64,
}

#[derive(Debug, Clone)]
struct PreparedPreflightInputs {
    loaded_profile: LoadedPrepareProfile,
    contract: PrepareInputContract,
    observations: Vec<PreparedInputObservation>,
    sampled_observations: Vec<PreparedInputObservation>,
    sampled_surfaces: Vec<PreparedSurfaceRecord>,
}

pub fn run_block_preflight(
    request: EntityBlockPreflightRequest<'_>,
) -> Result<EntityBlockPreflightReport, Refusal> {
    validate_sample_pct(request.sample_pct)?;
    let rows_bytes = fs::read(request.rows).map_err(|error| {
        EntityRefusalKind::InputContract.to_refusal(
            "Failed to read entity block preflight rows",
            json!({
                "stage": PREFLIGHT_STAGE,
                "path": request.rows.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Provide readable entity rows before running block preflight".to_string()),
        )
    })?;
    let runtime_config = load_block_runtime_config(request.strategy)?;
    let strategy = load_strategy_reference(request.strategy)?;
    let prepared = prepare_preflight_inputs(request)?;
    let posting_index = build_preflight_posting_index(&prepared.sampled_surfaces)?;
    let ngram_index = build_preflight_ngram_index(&prepared.sampled_surfaces)?;
    let core_view_name = entity_index::core_view_name(&prepared.contract.profile.id);
    let operators = default_block_candidate_operators(core_view_name);
    let block_result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: prepared.contract.profile.id.clone(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: preflight_generation_budget(),
        operators: operators.clone(),
    })?;
    let operator_reports = operator_reports(
        &operators,
        &block_result.diagnostics,
        &block_result.candidates,
        request.sample_pct,
    );
    let top_blocks = top_blocks(
        &prepared.sampled_surfaces,
        &posting_index,
        &ngram_index,
        request.sample_pct,
        DEFAULT_TOP_BLOCKS,
    )?;
    let totals = totals_from_diagnostics(&block_result.diagnostics, request.sample_pct);
    let budget_verdict = budget_verdict(&runtime_config, &totals, &operator_reports, &top_blocks);
    let input_row_count = prepared.observations.len() as u64;
    let sampled_row_count = prepared.sampled_observations.len() as u64;
    let surface_count = full_surface_count(request.rows, &prepared)?;
    let sampled_surface_count = prepared.sampled_surfaces.len() as u64;
    let mut report = EntityBlockPreflightReport {
        version: CANON_ENTITY_BLOCK_PREFLIGHT_VERSION.to_string(),
        rows: BlockPreflightInputReference {
            source: request.rows.display().to_string(),
            content_hash: witness::hash_bytes(&rows_bytes),
            row_count: input_row_count,
        },
        profile: prepared.contract.profile,
        strategy,
        sample: BlockPreflightSampleReport {
            requested_pct: request.sample_pct,
            exact: request.sample_pct == 100,
            hash_modulus: SAMPLE_MODULUS,
            hash_threshold: sample_threshold(request.sample_pct),
            input_row_count,
            sampled_row_count,
            surface_count,
            sampled_surface_count,
        },
        configured_budgets: BlockPreflightBudgetConfigReport::from(&runtime_config),
        budget_verdict,
        totals,
        operators: operator_reports,
        top_blocks,
        artifact_path: None,
    };
    if let Some(work_dir) = request.work_dir {
        let artifact_path = write_preflight_artifact(work_dir, &mut report)?;
        report.artifact_path = Some(artifact_path.display().to_string());
    }
    Ok(report)
}

pub fn render_block_preflight_summary(report: &EntityBlockPreflightReport) -> String {
    let mut lines = vec![
        format!(
            "entity block preflight: {}",
            budget_status_label(report.budget_verdict.status)
        ),
        format!(
            "rows: {} sampled: {} surfaces: {} sampled_surfaces: {}",
            report.sample.input_row_count,
            report.sample.sampled_row_count,
            report.sample.surface_count,
            report.sample.sampled_surface_count
        ),
        format!(
            "candidate_pairs: observed={} estimated={} max_surface={} max_operator={}",
            report.totals.observed_candidate_pairs_emitted,
            report.totals.estimated_candidate_pairs_emitted,
            report.totals.estimated_max_candidates_for_surface,
            report.totals.estimated_max_candidates_for_operator
        ),
    ];
    if let Some(top) = report.top_blocks.first() {
        lines.push(format!(
            "top_block: {} {}={} rows={}",
            top.operator_id, top.key_kind, top.key_value, top.estimated_row_count
        ));
    }
    lines.join("\n")
}

fn prepare_preflight_inputs(
    request: EntityBlockPreflightRequest<'_>,
) -> Result<PreparedPreflightInputs, Refusal> {
    let loaded_profile = load_prepare_profile_with_hash(request.profile)?;
    let contract = prepare_contract_for_loaded_profile(&loaded_profile)?;
    let observations = project_prepare_path(request.rows, &contract)?;
    let sampled_observations = sample_observations(&observations, request.sample_pct)?;
    let sampled_surfaces = prepare_surface_records_for_loaded_profile(
        request.rows,
        &loaded_profile,
        &sampled_observations,
    )?;
    Ok(PreparedPreflightInputs {
        loaded_profile,
        contract,
        observations,
        sampled_observations,
        sampled_surfaces,
    })
}

fn full_surface_count(rows: &Path, prepared: &PreparedPreflightInputs) -> Result<u64, Refusal> {
    if prepared.observations.len() == prepared.sampled_observations.len() {
        return Ok(prepared.sampled_surfaces.len() as u64);
    }
    Ok(prepare_surface_records_for_loaded_profile(
        rows,
        &prepared.loaded_profile,
        &prepared.observations,
    )?
    .len() as u64)
}

fn build_preflight_posting_index(
    surfaces: &[PreparedSurfaceRecord],
) -> Result<EntityPostingIndex, Refusal> {
    EntityPostingIndex::build(
        &entity_index::posting_surfaces(surfaces),
        EntityPostingBuildConfig {
            common_posting_limit: DEFAULT_INDEX_COMMON_POSTING_LIMIT,
        },
    )
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to build block preflight posting index",
            json!({
                "stage": PREFLIGHT_STAGE,
                "error": format!("{error:?}"),
                "writes_performed": false
            }),
            None,
        )
    })
}

fn build_preflight_ngram_index(
    surfaces: &[PreparedSurfaceRecord],
) -> Result<EntityNgramIndex, Refusal> {
    EntityNgramIndex::build(
        &entity_index::ngram_surfaces(surfaces),
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(DEFAULT_INDEX_NGRAM_WIDTH).ok_or_else(|| {
                EntityRefusalKind::ArtifactContract.to_refusal(
                    "Invalid block preflight n-gram width",
                    json!({
                        "stage": PREFLIGHT_STAGE,
                        "field": "ngram_width",
                        "actual": DEFAULT_INDEX_NGRAM_WIDTH,
                        "writes_performed": false
                    }),
                    None,
                )
            })?,
            common_posting_limit: DEFAULT_INDEX_COMMON_POSTING_LIMIT,
        },
    )
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to build block preflight n-gram index",
            json!({
                "stage": PREFLIGHT_STAGE,
                "error": format!("{error:?}"),
                "writes_performed": false
            }),
            None,
        )
    })
}

fn validate_sample_pct(sample_pct: u8) -> Result<(), Refusal> {
    if (1..=100).contains(&sample_pct) {
        return Ok(());
    }
    Err(EntityRefusalKind::InputContract.to_refusal(
        "Block preflight sample percentage must be between 1 and 100",
        json!({
            "stage": PREFLIGHT_STAGE,
            "field": "sample_pct",
            "actual": sample_pct,
            "minimum": 1,
            "maximum": 100,
            "writes_performed": false
        }),
        Some("Rerun canon entity block preflight with --sample-pct between 1 and 100".to_string()),
    ))
}

fn sample_observations(
    observations: &[PreparedInputObservation],
    sample_pct: u8,
) -> Result<Vec<PreparedInputObservation>, Refusal> {
    if sample_pct == 100 {
        return Ok(observations.to_vec());
    }
    let threshold = sample_threshold(sample_pct);
    let mut sampled = Vec::new();
    for observation in observations {
        if sample_bucket(observation)? < threshold {
            sampled.push(observation.clone());
        }
    }
    Ok(sampled)
}

fn sample_threshold(sample_pct: u8) -> u64 {
    u64::from(sample_pct).saturating_mul(SAMPLE_MODULUS / 100)
}

#[derive(Serialize)]
struct SampleObservationMaterial<'a> {
    profile_id: &'a str,
    primary_surface: &'a PreparedSurface,
    alias_surfaces: &'a [PreparedSurface],
    mention_surfaces: &'a [PreparedSurface],
    anchors: &'a [PreparedAnchor],
    context: &'a BTreeMap<String, Value>,
    provenance: &'a BTreeMap<String, String>,
}

fn sample_bucket(observation: &PreparedInputObservation) -> Result<u64, Refusal> {
    let material = SampleObservationMaterial {
        profile_id: &observation.profile_id,
        primary_surface: &observation.primary_surface,
        alias_surfaces: &observation.alias_surfaces,
        mention_surfaces: &observation.mention_surfaces,
        anchors: &observation.anchors,
        context: &observation.context,
        provenance: &observation.provenance,
    };
    let bytes = serde_json::to_vec(&material).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash block preflight sample material",
            json!({
                "stage": PREFLIGHT_STAGE,
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    let digest = blake3::hash(&bytes);
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&digest.as_bytes()[..8]);
    Ok(u64::from_be_bytes(prefix) % SAMPLE_MODULUS)
}

fn preflight_generation_budget() -> BlockCandidateBudgetConfig {
    BlockCandidateBudgetConfig::new(u64::MAX, u64::MAX, u64::MAX)
}

fn totals_from_diagnostics(
    diagnostics: &BlockCandidateGenerationDiagnostics,
    sample_pct: u8,
) -> BlockPreflightTotals {
    BlockPreflightTotals {
        observed_candidate_record_count: diagnostics.candidate_record_count,
        estimated_candidate_record_count: scale_pair_count(
            diagnostics.candidate_record_count,
            sample_pct,
        ),
        observed_candidate_pairs_emitted: diagnostics.candidate_pairs_emitted,
        estimated_candidate_pairs_emitted: scale_pair_count(
            diagnostics.candidate_pairs_emitted,
            sample_pct,
        ),
        observed_candidate_pairs_suppressed_by_cap: diagnostics.candidate_pairs_suppressed_by_cap,
        estimated_candidate_pairs_suppressed_by_cap: scale_pair_count(
            diagnostics.candidate_pairs_suppressed_by_cap,
            sample_pct,
        ),
        observed_suppressed_candidate_count: diagnostics.suppressed_candidate_count,
        estimated_suppressed_candidate_count: scale_pair_count(
            diagnostics.suppressed_candidate_count,
            sample_pct,
        ),
        observed_large_buckets_suppressed: diagnostics.large_buckets_suppressed,
        estimated_large_buckets_suppressed: scale_row_count(
            diagnostics.large_buckets_suppressed,
            sample_pct,
        ),
        observed_max_candidates_for_surface: diagnostics.max_candidates_for_surface,
        estimated_max_candidates_for_surface: scale_pair_count(
            diagnostics.max_candidates_for_surface,
            sample_pct,
        ),
        observed_max_candidates_for_operator: diagnostics.max_candidates_for_operator,
        estimated_max_candidates_for_operator: scale_pair_count(
            diagnostics.max_candidates_for_operator,
            sample_pct,
        ),
        candidate_pairs_per_surface_p50: scale_pair_count(
            diagnostics.candidate_pairs_per_surface_p50,
            sample_pct,
        ),
        candidate_pairs_per_surface_p95: scale_pair_count(
            diagnostics.candidate_pairs_per_surface_p95,
            sample_pct,
        ),
        candidate_pairs_per_surface_p99: scale_pair_count(
            diagnostics.candidate_pairs_per_surface_p99,
            sample_pct,
        ),
    }
}

fn operator_reports(
    operators: &[BlockCandidateOperator],
    diagnostics: &BlockCandidateGenerationDiagnostics,
    candidates: &[BlockCandidateRecord],
    sample_pct: u8,
) -> Vec<BlockPreflightOperatorReport> {
    let diagnostics_by_operator = diagnostics
        .operator_diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.operator_id.as_str(), diagnostic))
        .collect::<BTreeMap<_, _>>();
    let candidate_pairs_by_operator = candidate_pairs_by_operator(candidates);
    let mut cumulative_pairs = BTreeSet::<CandidatePairKey>::new();
    let mut reports = Vec::new();
    for operator in operators {
        let operator_id = operator_id(operator);
        let diagnostic = diagnostics_by_operator
            .get(operator_id)
            .map(|diagnostic| (*diagnostic).clone())
            .unwrap_or_else(|| empty_operator_diagnostic(operator_id));
        let pairs = candidate_pairs_by_operator
            .get(operator_id)
            .cloned()
            .unwrap_or_default();
        let marginal_count = pairs.difference(&cumulative_pairs).count() as u64;
        cumulative_pairs.extend(pairs);
        let cumulative_count = cumulative_pairs.len() as u64;
        reports.push(BlockPreflightOperatorReport {
            operator_id: operator_id.to_string(),
            observed_input_candidate_count: diagnostic.input_candidate_count,
            estimated_input_candidate_count: scale_pair_count(
                diagnostic.input_candidate_count,
                sample_pct,
            ),
            observed_eligible_candidate_count: diagnostic.eligible_candidate_count,
            estimated_eligible_candidate_count: scale_pair_count(
                diagnostic.eligible_candidate_count,
                sample_pct,
            ),
            observed_emitted_candidate_count: diagnostic.emitted_candidate_count,
            estimated_emitted_candidate_count: scale_pair_count(
                diagnostic.emitted_candidate_count,
                sample_pct,
            ),
            observed_suppressed_candidate_count: diagnostic.suppressed_candidate_count,
            estimated_suppressed_candidate_count: scale_pair_count(
                diagnostic.suppressed_candidate_count,
                sample_pct,
            ),
            observed_large_posting_suppressed_count: diagnostic.large_posting_suppressed_count,
            estimated_large_posting_suppressed_count: scale_row_count(
                diagnostic.large_posting_suppressed_count,
                sample_pct,
            ),
            observed_marginal_candidate_count: marginal_count,
            estimated_marginal_candidate_count: scale_pair_count(marginal_count, sample_pct),
            observed_cumulative_candidate_count: cumulative_count,
            estimated_cumulative_candidate_count: scale_pair_count(cumulative_count, sample_pct),
        });
    }
    reports
}

type CandidatePairKey = (String, String);

fn candidate_pairs_by_operator(
    candidates: &[BlockCandidateRecord],
) -> BTreeMap<String, BTreeSet<CandidatePairKey>> {
    let mut by_operator = BTreeMap::<String, BTreeSet<CandidatePairKey>>::new();
    for candidate in candidates {
        let pair = ordered_surface_pair(&candidate.left_surface_id, &candidate.right_surface_id);
        for hit in &candidate.block_hits {
            by_operator
                .entry(hit.operator_id.clone())
                .or_default()
                .insert(pair.clone());
        }
    }
    by_operator
}

fn ordered_surface_pair(left: &str, right: &str) -> CandidatePairKey {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn operator_id(operator: &BlockCandidateOperator) -> &str {
    match operator {
        BlockCandidateOperator::NgramTopK(config) => config.operator_id.as_str(),
        BlockCandidateOperator::RareTokenOverlap(config) => config.operator_id.as_str(),
        BlockCandidateOperator::AliasPatchMatch(config) => config.operator_id.as_str(),
    }
}

fn empty_operator_diagnostic(operator_id: &str) -> BlockOperatorCandidateDiagnostics {
    BlockOperatorCandidateDiagnostics {
        operator_id: operator_id.to_string(),
        input_candidate_count: 0,
        eligible_candidate_count: 0,
        emitted_candidate_count: 0,
        suppressed_candidate_count: 0,
        large_posting_suppressed_count: 0,
    }
}

fn top_blocks(
    surfaces: &[PreparedSurfaceRecord],
    posting_index: &EntityPostingIndex,
    ngram_index: &EntityNgramIndex,
    sample_pct: u8,
    top_k: usize,
) -> Result<Vec<BlockPreflightTopBlock>, Refusal> {
    let mut blocks = Vec::new();
    blocks.extend(exact_top_blocks(surfaces, sample_pct));
    blocks.extend(layout_top_blocks(
        "rare_token_overlap:run",
        "tfidf_term",
        PostingFeatureKind::TfidfTerm,
        &posting_index.tfidf_layout,
        sample_pct,
    )?);
    blocks.extend(layout_top_blocks(
        "ngram_topk:run",
        "ngram",
        PostingFeatureKind::Ngram,
        &ngram_index.ngram_layout,
        sample_pct,
    )?);
    sort_top_blocks(&mut blocks);
    blocks.truncate(top_k);
    Ok(blocks)
}

fn exact_top_blocks(
    surfaces: &[PreparedSurfaceRecord],
    sample_pct: u8,
) -> Vec<BlockPreflightTopBlock> {
    let mut buckets = BTreeMap::<(String, String), ExactBucketCounts>::new();
    let placeholders = placeholder_bucket_values();
    for surface in surfaces {
        let view_name = entity_index::core_view_name(&surface.profile_id);
        let key_value = entity_index::core_view_value(&surface.profile_id, surface);
        if key_value.trim().is_empty() || placeholders.contains(key_value.as_str()) {
            continue;
        }
        let counts = buckets
            .entry((format!("exact_view:{view_name}"), key_value))
            .or_default();
        counts.surface_count = counts.surface_count.saturating_add(1);
        counts.row_count = counts.row_count.saturating_add(surface.row_count);
    }
    buckets
        .into_iter()
        .map(
            |((operator_id, key_value), counts)| BlockPreflightTopBlock {
                operator_id,
                key_kind: "exact_view".to_string(),
                key_value,
                observed_surface_count: counts.surface_count,
                estimated_surface_count: scale_row_count(counts.surface_count, sample_pct),
                observed_row_count: counts.row_count,
                estimated_row_count: scale_row_count(counts.row_count, sample_pct),
            },
        )
        .collect()
}

#[derive(Debug, Clone, Default)]
struct ExactBucketCounts {
    surface_count: u64,
    row_count: u64,
}

fn layout_top_blocks(
    operator_id: &str,
    key_kind: &str,
    feature_kind: PostingFeatureKind,
    layout: &crate::entity::postings::PostingLayout,
    sample_pct: u8,
) -> Result<Vec<BlockPreflightTopBlock>, Refusal> {
    layout
        .dictionary
        .iter()
        .map(|entry| {
            let postings = layout.postings_for_term(entry.term_id).map_err(|error| {
                EntityRefusalKind::ArtifactContract.to_refusal(
                    "Failed to read block preflight postings",
                    json!({
                        "stage": PREFLIGHT_STAGE,
                        "operator_id": operator_id,
                        "term_id": entry.term_id,
                        "error": format!("{error:?}"),
                        "writes_performed": false
                    }),
                    None,
                )
            })?;
            let observed = postings.len() as u64;
            Ok(BlockPreflightTopBlock {
                operator_id: operator_id.to_string(),
                key_kind: feature_kind_label(feature_kind, key_kind).to_string(),
                key_value: entry.key.clone(),
                observed_surface_count: observed,
                estimated_surface_count: scale_row_count(observed, sample_pct),
                observed_row_count: observed,
                estimated_row_count: scale_row_count(observed, sample_pct),
            })
        })
        .collect()
}

fn feature_kind_label(feature_kind: PostingFeatureKind, key_kind: &str) -> &str {
    match feature_kind {
        PostingFeatureKind::ExactView => "exact_view",
        PostingFeatureKind::Token => "token",
        PostingFeatureKind::Ngram => "ngram",
        PostingFeatureKind::TfidfTerm => key_kind,
    }
}

fn sort_top_blocks(blocks: &mut [BlockPreflightTopBlock]) {
    blocks.sort_by(|left, right| {
        right
            .estimated_row_count
            .cmp(&left.estimated_row_count)
            .then_with(|| {
                right
                    .estimated_surface_count
                    .cmp(&left.estimated_surface_count)
            })
            .then_with(|| left.operator_id.cmp(&right.operator_id))
            .then_with(|| left.key_kind.cmp(&right.key_kind))
            .then_with(|| left.key_value.cmp(&right.key_value))
    });
}

fn budget_verdict(
    config: &BlockRuntimeConfig,
    totals: &BlockPreflightTotals,
    operators: &[BlockPreflightOperatorReport],
    top_blocks: &[BlockPreflightTopBlock],
) -> BlockPreflightBudgetVerdict {
    let max_operator = operators
        .iter()
        .max_by(|left, right| {
            left.estimated_emitted_candidate_count
                .cmp(&right.estimated_emitted_candidate_count)
                .then_with(|| right.operator_id.cmp(&left.operator_id))
        })
        .map(|operator| {
            (
                operator.operator_id.clone(),
                operator.observed_emitted_candidate_count,
                operator.estimated_emitted_candidate_count,
            )
        });
    let max_exact = top_blocks
        .iter()
        .filter(|block| block.key_kind == "exact_view")
        .max_by(|left, right| {
            left.estimated_row_count
                .cmp(&right.estimated_row_count)
                .then_with(|| right.key_value.cmp(&left.key_value))
        })
        .map(|block| {
            (
                block.key_value.clone(),
                block.observed_row_count,
                block.estimated_row_count,
            )
        });
    let checks = vec![
        BlockPreflightBudgetCheck::new(
            "block.max_candidates_per_surface",
            totals.observed_max_candidates_for_surface,
            totals.estimated_max_candidates_for_surface,
            config.candidate_budget.max_candidates_per_surface,
            "surface",
            None,
        ),
        BlockPreflightBudgetCheck::new(
            "block.max_candidates_per_operator",
            max_operator
                .as_ref()
                .map(|(_, observed, _)| *observed)
                .unwrap_or(0),
            max_operator
                .as_ref()
                .map(|(_, _, estimated)| *estimated)
                .unwrap_or(0),
            config.candidate_budget.max_candidates_per_operator,
            "operator",
            max_operator.map(|(operator_id, _, _)| operator_id),
        ),
        BlockPreflightBudgetCheck::new(
            "block.max_candidates_per_run",
            totals.observed_candidate_pairs_emitted,
            totals.estimated_candidate_pairs_emitted,
            config.candidate_budget.max_candidates_per_run,
            "run",
            None,
        ),
        BlockPreflightBudgetCheck::new(
            "block.max_exact_bucket_size",
            max_exact
                .as_ref()
                .map(|(_, observed, _)| *observed)
                .unwrap_or(0),
            max_exact
                .as_ref()
                .map(|(_, _, estimated)| *estimated)
                .unwrap_or(0),
            config.max_exact_bucket_size,
            "exact_bucket",
            max_exact.map(|(key_value, _, _)| key_value),
        ),
    ];
    let status = checks
        .iter()
        .map(|check| check.status)
        .max()
        .unwrap_or(BlockPreflightBudgetStatus::Pass);
    BlockPreflightBudgetVerdict { status, checks }
}

impl BlockPreflightBudgetCheck {
    fn new(
        policy_id: impl Into<String>,
        observed: u64,
        estimated: u64,
        configured: u64,
        subject_kind: impl Into<String>,
        subject_id: Option<String>,
    ) -> Self {
        Self {
            policy_id: policy_id.into(),
            status: budget_status(estimated, configured),
            observed,
            estimated,
            configured,
            subject_kind: subject_kind.into(),
            subject_id,
        }
    }
}

fn budget_status(estimated: u64, configured: u64) -> BlockPreflightBudgetStatus {
    if estimated > configured {
        return BlockPreflightBudgetStatus::WouldRefuse;
    }
    if configured > 0 && u128::from(estimated).saturating_mul(10) >= u128::from(configured) * 9 {
        return BlockPreflightBudgetStatus::Tight;
    }
    BlockPreflightBudgetStatus::Pass
}

fn budget_status_label(status: BlockPreflightBudgetStatus) -> &'static str {
    match status {
        BlockPreflightBudgetStatus::Pass => "pass",
        BlockPreflightBudgetStatus::Tight => "tight",
        BlockPreflightBudgetStatus::WouldRefuse => "would-refuse",
    }
}

fn scale_pair_count(value: u64, sample_pct: u8) -> u64 {
    if sample_pct == 100 {
        return value;
    }
    let pct = u128::from(sample_pct);
    let denominator = pct.saturating_mul(pct);
    scale_count(value, 10_000, denominator)
}

fn scale_row_count(value: u64, sample_pct: u8) -> u64 {
    if sample_pct == 100 {
        return value;
    }
    scale_count(value, 100, u128::from(sample_pct))
}

fn scale_count(value: u64, numerator: u128, denominator: u128) -> u64 {
    if value == 0 || denominator == 0 {
        return value;
    }
    let scaled = u128::from(value)
        .saturating_mul(numerator)
        .saturating_add(denominator - 1)
        / denominator;
    u64::try_from(scaled).unwrap_or(u64::MAX)
}

fn load_strategy_reference(strategy: &Path) -> Result<EntityStrategyReference, Refusal> {
    let bytes = fs::read(strategy).map_err(|error| {
        EntityRefusalKind::Strategy.to_refusal(
            "Failed to read entity block preflight strategy",
            json!({
                "stage": PREFLIGHT_STAGE,
                "path": strategy.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Provide a readable strategy YAML file".to_string()),
        )
    })?;
    let value = serde_yaml::from_slice::<serde_yaml::Value>(&bytes).map_err(|error| {
        EntityRefusalKind::Strategy.to_refusal(
            "Failed to parse entity block preflight strategy",
            json!({
                "stage": PREFLIGHT_STAGE,
                "path": strategy.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Fix the strategy YAML before rerunning block preflight".to_string()),
        )
    })?;
    let id = yaml_string(&value, "strategy_id")
        .or_else(|| yaml_string(&value, "id"))
        .unwrap_or_else(|| {
            strategy
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("entity_block_strategy")
                .to_string()
        });
    let version = yaml_string(&value, "strategy_version")
        .or_else(|| yaml_string(&value, "version"))
        .unwrap_or_else(|| "v0".to_string());
    Ok(EntityStrategyReference {
        id,
        version,
        content_hash: witness::hash_bytes(&bytes),
    })
}

fn yaml_string(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String(key.to_string())))
        .and_then(serde_yaml::Value::as_str)
        .map(ToOwned::to_owned)
}

fn write_preflight_artifact(
    work_dir: &Path,
    report: &mut EntityBlockPreflightReport,
) -> Result<PathBuf, Refusal> {
    let metadata = fs::metadata(work_dir).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Block preflight work directory must already exist",
            json!({
                "stage": PREFLIGHT_STAGE,
                "work_dir": work_dir.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Create the work directory or omit --work-dir for a read-only report".to_string()),
        )
    })?;
    if !metadata.is_dir() {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Block preflight work-dir is not a directory",
            json!({
                "stage": PREFLIGHT_STAGE,
                "work_dir": work_dir.display().to_string(),
                "writes_performed": false
            }),
            Some("Pass a directory path to --work-dir or omit it".to_string()),
        ));
    }
    let path = work_dir.join("block_preflight.json");
    report.artifact_path = Some(path.display().to_string());
    let bytes = serde_json::to_vec(report).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to serialize block preflight artifact",
            json!({
                "stage": PREFLIGHT_STAGE,
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    fs::write(&path, bytes).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to write block preflight artifact",
            json!({
                "stage": PREFLIGHT_STAGE,
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(
                "Check --work-dir permissions or omit --work-dir for stdout-only preflight"
                    .to_string(),
            ),
        )
    })?;
    Ok(path)
}

fn placeholder_bucket_values() -> BTreeSet<&'static str> {
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
    .collect()
}
