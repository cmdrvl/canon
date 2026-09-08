#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_REQUEST_VERSION,
    CANON_GEO_DEED_INDEX_ROWS_VERSION, CANON_GEO_DEED_TRUTH_VERSION,
    CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION, CANON_GEO_H7_POPULATION_ROWS_VERSION,
    CANON_GEO_H7_POPULATION_VERSION, CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
    CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION,
    CANON_GEO_PLAN_VERSION, CANON_GEO_POINT_POPULATION_VERSION, CANON_GEO_RETRY_LOOP_VERSION,
    CANON_GEO_RETRY_RECOVERY_VERSION, CANON_GEO_RUN_VERSION, GEO_MATERIALIZE_HOME_CELLS_COMMAND,
    GEO_RETRY_LOOP_BINDING_ID, GEO_RETRY_LOOP_OUTPUT_ID, GEO_RETRY_PASS_STAGE_COMMAND,
    GEO_RETRY_RECEIPT_BINDING_ID, GEO_RETRY_RUN_BINDING_ID, GEO_ROWS_BINDING_ID,
    GeoAcquisitionCounts, GeoAcquisitionDenominator, GeoAcquisitionProofClass,
    GeoAcquisitionReceipt, GeoAcquisitionResumability, GeoAcquisitionTerminalState,
    GeoAdjudicationLabel, GeoAdjudicationRetainedLabelRow, GeoBoundedGeography, GeoBoundedSubset,
    GeoClaimClass, GeoControlEntityLevel, GeoDeedIndexRowsRequest, GeoDeedTruthLoanRef,
    GeoDenominatorSource, GeoDigest, GeoDigestAlgorithm, GeoEntityLevel, GeoEvidenceClass,
    GeoExecutorKind, GeoExecutorTrace, GeoFieldRole, GeoH7PipBlockPopulationBatchRequest,
    GeoH7PopulationRowsRequest, GeoH7StagingSourceRecordBytesBatchRequest, GeoHomeCellRow,
    GeoHomeCellRowsRequest, GeoIdentityParticipation, GeoImageTilePin, GeoLocalArtifactDigest,
    GeoNativeEntityScope, GeoNullOrdering, GeoOrderDirection, GeoOrderingTerm,
    GeoPaginationReceipt, GeoPaginationRequest, GeoPlan, GeoPlanArtifactRef, GeoPlanBudgetRef,
    GeoPlanClaimEffect, GeoPlanGrainOutcome, GeoPlanGrainStatus, GeoPlanInventoryRef,
    GeoPlanNodeOverlay, GeoPlanProfileRef, GeoPlanStage, GeoPlanStatus, GeoPlanTransitionSet,
    GeoPointPopulationArtifact, GeoPointPopulationPoint, GeoReleasePin, GeoRequestedField,
    GeoRetryLoopArtifact, GeoRetryPolicy, GeoRetryTerminal, GeoRowByteCeilings, GeoRun,
    GeoRunArtifactBinding, GeoRunStatus, GeoSourceRelease, GeoSubsetPredicate,
    GeoSubsetPredicateKind, GeoTileSourceBinding, GeoTruthPlane, GeoValidTimeInterval,
    canonical_deed_truth_bytes, canonical_geo_acquisition_request_bytes, canonical_geo_run_bytes,
    canonical_h7_population_bytes, canonical_retry_loop_bytes, canonical_retry_recovery_bytes,
    derive_deed_truth_from_index, geo_acquisition_request_id,
    geo_acquisition_request_semantic_hash, geo_plan_semantic_hash,
    materialize_h7_pip_block_population_batch, materialize_h7_population_rows,
    materialize_h7_staging_source_record_bytes_batch, measure_recovery,
    revalidate_adjudication_labels, run_geo_plan, validate_geo_acquisition_receipt,
    validate_geo_acquisition_request, validate_geo_plan, validate_geo_run,
    validate_point_population_artifact, validate_retry_loop_artifact,
};
use canon::project::{
    ProjectExtensionDagNode, ProjectExtensionDagOutput, ProjectExtensionDagRequest,
    ProjectPlanErrorCode, ProjectPlanNodeClass, ProjectPlanNodeKind,
    ProjectPlanOutputMaterialization, ProjectPlanRefusalCondition, ProjectPlanSideEffect,
    ProjectPlanSideEffectKind, ProjectRunFailurePolicy, ProjectRunNodeOutcome, ProjectRunPolicy,
    compile_extension_project_plan, digest_bytes as project_digest_bytes,
};
use chrono::{DateTime, NaiveDate};
use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    io::{self, Write as _},
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

const MANIFEST_VERSION: &str = "canon_geo_measurement_manifest.v0";
const PLAN_VERSION: &str = "canon_geo_measurement_plan.v0";
const REPORT_VERSION: &str = "canon_geo_measurement_report.v0";
const RECEIPTS_VERSION: &str = "canon_geo_measurement_receipts.v0";
const RESULT_ARTIFACT_VERSION: &str = "canon_geo_measurement_result_artifact.v0";
const RESULT_SET_VERSION: &str = "canon_geo_measurement_result_set.v0";
const D0_ADJUDICATION_LABELS_VERSION: &str = "canon_geo_d0_adjudication_labels.v0";
const D0_ADJUDICATION_PINS_VERSION: &str = "canon_geo_d0_adjudication_pins.v0";
const EXECUTION_CHANNEL: &str = "cmdrvl_data_mcp";
const EXECUTION_TRANSFORM: &str = "cmdrvl_data_sqlglot_normalized_plus_tool_row_limit";
const LIVENESS_NOT_ATTESTED: &str = "receipt is internally consistent, but this offline runner does not attest liveness, authenticity, or query-history provenance";
const QUERY_HISTORY_BOUND: &str = "cmdrvl_data_live receipt query_id is bound to local query history statement text by executed_query_text_sha256; this attests query correspondence only, not warehouse liveness or authenticity";
const CLAIM_BOUNDARY: &str = "Offline receipt consistency validation only. A receipt_consistent row means the receipt is bound to result artifact bytes and executed query text bytes, and matches the manifest's declared offline checks. source_sql_sha256 is the local file byte digest; executed_query_text_sha256 is recomputed from the supplied normalized query text artifact after the declared cmdrvl-data/Snowflake transform. result_set_sha256 is over an unordered canonical result set sorted deterministically by compact JSON row encoding. cmdrvl_data_live receipts require --query-history correspondence before LiveComplete attestation; missing history stays query_history_unattested and mismatched query text is malformed. This proves byte integrity and query correspondence, not authenticity or liveness. Integration-test positive JSON is a contract fixture, not live proof of cmdrvl-data execution.";
const PROVIDER_RESPONSE_BYTES_DIGEST_ID: &str = "provider_response_bytes";
const GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID: &str = "geocode_candidate_rows";
const G4_RETRY_RECOVERY_DENOMINATOR: usize = 40;
const DEFAULT_G4_RETRY_RECOVERY_MAX_BYTES: u64 = 131_072;
const RETRY_RECOVERY_HOME_CELL_STAGE_NODE_ID: &str = "geo.retry_recovery.home_cells";
const RETRY_RECOVERY_PASS_STAGE_NODE_ID: &str = "geo.retry_recovery.retry_pass";
const REQUIRED_CORE_MEASUREMENT_IDS: &[&str] = &[
    "appendix_b_centroid_percolation",
    "appendix_c_r8_density",
    "appendix_d_same_cell_predicates",
    "appendix_d_candidate_reach",
    "appendix_d_stratified_halo_centers",
    "appendix_d_stratified_halo",
    "appendix_f_overture_three_source",
];

#[derive(Debug, Parser)]
#[command(
    name = "canon_geo_measurements",
    about = "Validate offline Canon Geo measurement manifests and cmdrvl-data receipts"
)]
struct Args {
    #[arg(long, default_value = "scripts/geo_measurements/manifest.json")]
    manifest: PathBuf,
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,
    #[arg(long)]
    receipts: Option<PathBuf>,
    #[arg(long)]
    query_history: Option<PathBuf>,
    #[arg(long, value_enum)]
    emit: Option<EmitMode>,
    #[command(subcommand)]
    command: Option<MeasurementCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum EmitMode {
    Plan,
    Report,
}

#[derive(Debug, Subcommand)]
enum MeasurementCommand {
    #[command(name = "derive-deed-truth")]
    DeedTruth(DeedTruthArgs),
    #[command(name = "revalidate-d0-adjudication")]
    RevalidateD0Adjudication(Box<RevalidateD0AdjudicationArgs>),
    #[command(name = "materialize-acquisition-receipt")]
    AcquisitionReceipt(Box<AcquisitionReceiptArgs>),
    #[command(name = "prepare-retry-recovery")]
    PrepareRetryRecovery(Box<PrepareRetryRecoveryArgs>),
    #[command(name = "record-retry-recovery-pass")]
    RecordRetryRecoveryPass(Box<RecordRetryRecoveryPassArgs>),
    #[command(name = "materialize-retry-recovery-run")]
    MaterializeRetryRecoveryRun(Box<MaterializeRetryRecoveryRunArgs>),
    #[command(name = "measure-retry-recovery")]
    RetryRecovery(RetryRecoveryArgs),
    #[command(name = "materialize-h7-population")]
    Population(H7PopulationArgs),
    #[command(name = "materialize-h7-staging-batch")]
    StagingBatch(H7BatchArgs),
    #[command(name = "materialize-h7-pip-block-batch")]
    PipBlockBatch(H7BatchArgs),
}

#[derive(Debug, ClapArgs)]
struct DeedTruthArgs {
    /// JSON array of deed-truth loan references
    #[arg(long)]
    loans: PathBuf,
    /// JSON file holding canon_geo_deed_index_rows.v0 rows
    #[arg(long)]
    deeds: PathBuf,
    /// Inclusive recording-date window in days after origination
    #[arg(long)]
    window_days: u32,
}

#[derive(Debug, ClapArgs)]
struct RevalidateD0AdjudicationArgs {
    /// Retained canon_geo_d0_adjudication_labels.v0 JSON file
    #[arg(long = "d0-labels", alias = "labels")]
    labels: PathBuf,
    /// Retained canon_geo_d0_adjudication_pins.v0 JSON file
    #[arg(long = "d0-pins", alias = "pins")]
    pins: PathBuf,
    /// Directory containing retained crop bytes named <case_id>.bin
    #[arg(long)]
    crop_dir: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct D0AdjudicationLabelsFile {
    version: String,
    labels: Vec<D0AdjudicationLabelRow>,
}

#[derive(Debug, Deserialize)]
struct D0AdjudicationLabelRow {
    case_id: String,
    subject_id: String,
    pin_id: String,
    window_blake3: String,
    candidate_parcel_ids: Vec<String>,
    overlay_geometry_blake3: String,
    crop_blake3: String,
    label: D0AdjudicationLabel,
    adjudicator_id: String,
    truth_plane: GeoTruthPlane,
    notes_blake3: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum D0AdjudicationLabel {
    Selected { selected_parcels: Vec<String> },
    Disposition(String),
}

#[derive(Debug, Deserialize)]
struct D0AdjudicationPinsFile {
    version: String,
    pins: Vec<D0AdjudicationPin>,
}

#[derive(Debug, Deserialize)]
struct D0AdjudicationPin {
    pin_id: String,
    source_dataset: String,
    url: String,
    byte_range: Option<(u64, u64)>,
    etag: Option<String>,
    blake3: Option<String>,
    vintage: String,
    license_id: String,
    license_text_blake3: String,
}

#[derive(Debug, ClapArgs)]
struct RetryRecoveryArgs {
    /// canon_geo_point_population.v0 file with the frozen gross-class denominator
    #[arg(long)]
    population: PathBuf,
    /// Directory of canon_geo_retry_loop.v0 JSON files
    #[arg(long)]
    loops: PathBuf,
    /// Directory of canon_geo_run.v0 JSON files keyed by run semantic hash
    #[arg(long)]
    runs: PathBuf,
    /// Directory of canon_geo_acquisition_receipt.v0 files plus retained byte sidecars
    #[arg(long)]
    receipts: PathBuf,
}

#[derive(Debug, ClapArgs)]
struct RecordRetryRecoveryPassArgs {
    /// Current canon_geo_retry_loop.v0 artifact
    #[arg(long = "loop")]
    retry_loop: PathBuf,
    /// Latest pinned canon_geo_run.v0 artifact to record into the retry loop
    #[arg(long)]
    latest_run: PathBuf,
    /// Matching canon_geo_acquisition_receipt.v0 artifact for the emitted request
    #[arg(long)]
    receipt: PathBuf,
    /// Workspace root for the internal geo run stage
    #[arg(long)]
    work_dir: PathBuf,
    /// File to receive the updated canon_geo_retry_loop.v0 artifact
    #[arg(long)]
    out_loop: PathBuf,
    /// File to receive the canon_geo_run.v0 manifest for the retry-pass stage
    #[arg(long)]
    out_run: PathBuf,
}

#[derive(Debug, ClapArgs)]
struct MaterializeRetryRecoveryRunArgs {
    /// canon_geo_point_population.v0 file with the frozen gross-class denominator
    #[arg(long)]
    population: PathBuf,
    /// Point id inside the frozen population to materialize
    #[arg(long)]
    point_id: String,
    /// Current canon_geo_retry_loop.v0 artifact whose request emitted the receipt
    #[arg(long = "loop")]
    retry_loop: PathBuf,
    /// Matching retained/live canon_geo_acquisition_receipt.v0 artifact
    #[arg(long)]
    receipt: PathBuf,
    /// Retained geocode candidate rows pinned by the receipt
    #[arg(long)]
    candidate_rows: PathBuf,
    /// Workspace root for the internal home-cell geo run stage
    #[arg(long)]
    work_dir: PathBuf,
    /// File to receive the canon_geo_run.v0 home-cell run manifest
    #[arg(long)]
    out_run: PathBuf,
    /// Optional file to receive the exact canon_geo_home_cell_rows.v1 input artifact
    #[arg(long)]
    out_home_cell_rows: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
struct AcquisitionReceiptArgs {
    /// canon_geo_acquisition_request.v0 emitted by the retry loop
    #[arg(long)]
    request: PathBuf,
    /// Raw provider response bytes retained by the external acquisition step
    #[arg(long)]
    provider_response_bytes: PathBuf,
    /// JSON array or object containing per-candidate geocode rows
    #[arg(long)]
    candidate_rows: PathBuf,
    /// Directory to receive the receipt JSON and retained sidecars
    #[arg(long)]
    out_dir: PathBuf,
    /// Receipt proof class; live is for a fresh provider call, retained is replay
    #[arg(long, value_enum, default_value = "live")]
    proof_class: AcquisitionReceiptProofArg,
    /// Required for retained proof, forbidden for live proof
    #[arg(long)]
    retained_receipt_id: Option<String>,
    #[arg(long, value_enum, default_value = "http-service")]
    executor_kind: AcquisitionExecutorKindArg,
    #[arg(long)]
    executor_id: String,
    #[arg(long)]
    executor_version: String,
    #[arg(long)]
    tool_id: String,
    #[arg(long)]
    tool_version: String,
    #[arg(long)]
    executor_request_id: String,
    #[arg(long)]
    executor_query_id: String,
    #[arg(long)]
    executor_attempt_id: Option<String>,
    /// Optional denominator count for the requested bounded subset
    #[arg(long)]
    denominator_count: Option<u64>,
}

#[derive(Debug, ClapArgs)]
struct PrepareRetryRecoveryArgs {
    /// canon_geo_point_population.v0 file with the frozen gross-class denominator
    #[arg(long)]
    population: PathBuf,
    /// JSON array of recovered clear addresses keyed by point_id/subject_id
    #[arg(long)]
    address_rows: PathBuf,
    /// Provider profile JSON used by the external acquisition helper
    #[arg(
        long,
        default_value = "scripts/geo_acquisition/providers/census_geocoder_current.json"
    )]
    provider_profile: PathBuf,
    /// Repository root for resolving the default provider profile
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,
    /// Directory to receive request JSON, empty loop seeds, and address files
    #[arg(long)]
    out_dir: PathBuf,
    /// Maximum retry passes to encode in the emitted loop policy
    #[arg(long, default_value_t = 2)]
    max_passes: u8,
    /// Maximum retained provider-response plus candidate-row bytes per request
    #[arg(long, default_value_t = DEFAULT_G4_RETRY_RECOVERY_MAX_BYTES)]
    max_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum AcquisitionReceiptProofArg {
    Retained,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum AcquisitionExecutorKindArg {
    Catalog,
    QueryEngine,
    ObjectStore,
    HttpService,
    LocalFile,
    ManualExport,
    Other,
}

#[derive(Debug, ClapArgs)]
struct H7PopulationArgs {
    /// JSON file holding canon_geo_h7_population_rows.v0 rows
    #[arg(long)]
    rows: PathBuf,
}

#[derive(Debug, ClapArgs)]
struct H7BatchArgs {
    /// JSON file holding an H.7 NYC measurement-profile batch
    #[arg(long)]
    batch: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: String,
    scope: String,
    offline_only: bool,
    required_measurement_ids: Vec<String>,
    measurements: Vec<ManifestMeasurement>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ManifestMeasurement {
    id: String,
    section: String,
    gate: String,
    geography: String,
    tier: String,
    declared_grain: String,
    description: String,
    sql_path: String,
    source_sql_sha256: String,
    execution_transform: String,
    as_of: String,
    expected_row_count: u64,
    release_pins: BTreeMap<String, String>,
    denominator_fields: Vec<String>,
    expected_denominators: BTreeMap<String, u64>,
    expected_sanity: BTreeMap<String, Value>,
    result_row_validation: String,
    limitations: Vec<String>,
    result_fields: Vec<String>,
    expected_result_rows: Vec<BTreeMap<String, Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptBundle {
    version: String,
    receipts: Vec<Receipt>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    measurement_id: String,
    source_sql_sha256: String,
    executed_query_text_sha256: String,
    release_pins: BTreeMap<String, String>,
    execution_channel: String,
    execution_transform: String,
    executed_query_text_path: String,
    query_id: Option<String>,
    executed_at: String,
    as_of: String,
    row_count: u64,
    proof_class: String,
    result_artifact_path: Option<String>,
    result_artifact_sha256: Option<String>,
    result_set_sha256: Option<String>,
    denominators: BTreeMap<String, u64>,
    sanity: BTreeMap<String, Value>,
    #[serde(default)]
    gate_values: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultArtifact {
    version: String,
    measurement_id: String,
    execution_channel: String,
    execution_transform: String,
    executed_query_text_path: String,
    query_id: Option<String>,
    source_sql_sha256: String,
    executed_query_text_sha256: String,
    rows: Vec<BTreeMap<String, Value>>,
}

#[derive(Debug)]
struct QueryHistoryLookup {
    unavailable: Option<String>,
    entries: BTreeMap<String, QueryHistoryEntry>,
    duplicate_query_ids: BTreeSet<String>,
}

#[derive(Debug)]
struct QueryHistoryEntry {
    statement_text_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryHistoryAttestation {
    NotRequired,
    Bound,
    Unattested,
    Refused,
}

enum QueryHistoryBinding {
    Bound,
    Unattested(String),
    Refused(String),
}

#[derive(Serialize)]
struct CanonicalResultSet<'a> {
    version: &'static str,
    measurement_id: &'a str,
    source_sql_sha256: &'a str,
    executed_query_text_sha256: &'a str,
    rows: Vec<BTreeMap<String, Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetryRecoveryAddressRow {
    #[serde(alias = "POINT_ID")]
    point_id: String,
    #[serde(alias = "SUBJECT_ID", alias = "PROPERTY_KEY", alias = "property_key")]
    subject_id: String,
    #[serde(alias = "ASSERTED_ADDRESS")]
    asserted_address: String,
    #[serde(alias = "ONE_LINE_ADDRESS")]
    one_line_address: String,
}

#[derive(Debug, Serialize)]
struct RetryRecoveryPreparationReport {
    population_id: String,
    denominator: u64,
    provider_id: String,
    provider_version: String,
    request_dir: String,
    loop_dir: String,
    address_dir: String,
    prepared: u64,
    request_semantic_hashes: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct RetryRecoveryPassRecordReport {
    subject_id: String,
    pass_index: u8,
    terminal: Option<GeoRetryTerminal>,
    project_node_id: String,
    stage_plan_id: String,
    stage_plan_semantic_hash: String,
    stage_run_id: String,
    stage_run_semantic_hash: String,
    retry_loop_output_path: String,
    out_loop: String,
    out_run: String,
}

#[derive(Debug, Serialize)]
struct RetryRecoveryRunMaterializationReport {
    point_id: String,
    subject_id: String,
    candidate_count: u64,
    project_node_id: String,
    stage_plan_id: String,
    stage_plan_semantic_hash: String,
    stage_run_id: String,
    stage_run_semantic_hash: String,
    stage_run_status: GeoRunStatus,
    singleton_home_cell_r9: Option<String>,
    home_cell_rows_output_path: Option<String>,
    out_run: String,
    precision_claim: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RetryRecoveryCandidateRow {
    #[serde(alias = "POINT_ID")]
    point_id: String,
    #[serde(default, alias = "SUBJECT_ID", alias = "subjectId")]
    subject_id: Option<String>,
    #[serde(alias = "CANDIDATE_RANK")]
    candidate_rank: u64,
    #[serde(alias = "LON_E7")]
    lon_e7: i64,
    #[serde(alias = "LAT_E7")]
    lat_e7: i64,
    #[serde(default, alias = "PROVIDER_ID")]
    provider_id: Option<String>,
    #[serde(default, alias = "PROVIDER_VERSION")]
    provider_version: Option<String>,
}

#[derive(Debug, Clone)]
struct RetryRecoveryCandidate {
    value: Value,
    row: RetryRecoveryCandidateRow,
}

#[derive(Debug, Serialize)]
struct MeasurementPlan {
    version: String,
    scope: String,
    offline_only: bool,
    execution: String,
    claim_boundary: String,
    measurements: Vec<MeasurementPlanRow>,
}

#[derive(Debug, Serialize)]
struct MeasurementPlanRow {
    order: usize,
    id: String,
    section: String,
    gate: String,
    geography: String,
    tier: String,
    declared_grain: String,
    sql_path: String,
    source_sql_sha256: String,
    execution_transform: String,
    as_of: String,
    release_pins: BTreeMap<String, String>,
    denominator_fields: Vec<String>,
    sanity_fields: Vec<String>,
    result_row_validation: String,
    limitations: Vec<String>,
    result_fields: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MeasurementReport {
    version: String,
    scope: String,
    offline_only: bool,
    execution: String,
    claim_boundary: String,
    summary: ReportSummary,
    measurements: Vec<MeasurementStatusRow>,
}

#[derive(Debug, Serialize)]
struct ReportSummary {
    total: usize,
    receipt_consistent: usize,
    query_history_unattested: usize,
    snapshot_moved: usize,
    result_mismatch: usize,
    malformed: usize,
    missing: usize,
}

#[derive(Debug, Serialize)]
struct MeasurementStatusRow {
    order: Option<usize>,
    measurement_id: String,
    status: MeasurementStatus,
    execution_channel: Option<String>,
    execution_transform: Option<String>,
    executed_query_text_path: Option<String>,
    query_id: Option<String>,
    executed_at: Option<String>,
    as_of: Option<String>,
    release_pins: Option<BTreeMap<String, String>>,
    declared_proof_class: Option<String>,
    proof_attestation: Option<String>,
    source_sql_sha256: Option<String>,
    executed_query_text_sha256: Option<String>,
    result_artifact_sha256: Option<String>,
    result_set_sha256: Option<String>,
    result_validation: Option<String>,
    row_count: Option<u64>,
    details: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MeasurementStatus {
    ReceiptConsistent,
    QueryHistoryUnattested,
    SnapshotMoved,
    ResultMismatch,
    Malformed,
    Missing,
}

#[derive(Debug)]
struct AppError {
    message: String,
}

impl AppError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AppError {}

fn main() -> ExitCode {
    match run() {
        Ok(exit) => exit,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, AppError> {
    let args = Args::parse();
    if let Some(command) = args.command {
        return run_measurement_command(command);
    }

    let repo_root = absolute_path(&args.repo_root)?;
    let manifest = load_manifest(&repo_root, &args.manifest)?;
    let emit = args.emit.unwrap_or(if args.receipts.is_some() {
        EmitMode::Report
    } else {
        EmitMode::Plan
    });

    match emit {
        EmitMode::Plan => {
            print_json(&plan_for(&manifest))?;
            Ok(ExitCode::SUCCESS)
        }
        EmitMode::Report => {
            let receipts_path = args.receipts.as_ref().ok_or_else(|| {
                AppError::new("--receipts is required when --emit report is selected")
            })?;
            let receipts = load_receipts(receipts_path)?;
            let query_history = load_query_history(args.query_history.as_deref());
            let receipt_base = receipts_path.parent().unwrap_or_else(|| Path::new("."));
            let report = report_for(&manifest, &receipts, receipt_base, &query_history);
            let ok = report.summary.snapshot_moved == 0
                && report.summary.result_mismatch == 0
                && report.summary.query_history_unattested == 0
                && report.summary.malformed == 0
                && report.summary.missing == 0;
            print_json(&report)?;
            Ok(if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
    }
}

fn run_measurement_command(command: MeasurementCommand) -> Result<ExitCode, AppError> {
    match command {
        MeasurementCommand::DeedTruth(args) => {
            let loans: Vec<GeoDeedTruthLoanRef> = load_unversioned_json(
                &args.loans,
                "deed-truth loans",
                "canon_geo_measurements derive-deed-truth --loans <LOANS.json>",
            )?;
            let deeds: GeoDeedIndexRowsRequest = load_json(
                &args.deeds,
                CANON_GEO_DEED_INDEX_ROWS_VERSION,
                "deeds",
                "canon_geo_measurements derive-deed-truth --deeds <DEEDS.json>",
            )?;
            let artifact = derive_deed_truth_from_index(&loans, &deeds, args.window_days)
                .map_err(|error| AppError::new(error.to_string()))?;
            let bytes = canonical_deed_truth_bytes(&artifact).map_err(|error| {
                AppError::new(format!(
                    "failed to serialize {CANON_GEO_DEED_TRUTH_VERSION}: {error}"
                ))
            })?;
            write_canonical(&bytes)?;
        }
        MeasurementCommand::RevalidateD0Adjudication(args) => {
            return revalidate_d0_adjudication(*args);
        }
        MeasurementCommand::AcquisitionReceipt(args) => {
            let receipt = materialize_acquisition_receipt(*args)?;
            print_json(&receipt)?;
        }
        MeasurementCommand::PrepareRetryRecovery(args) => {
            let report = prepare_retry_recovery(*args)?;
            print_json(&report)?;
        }
        MeasurementCommand::RecordRetryRecoveryPass(args) => {
            let report = record_retry_recovery_pass(*args)?;
            print_json(&report)?;
        }
        MeasurementCommand::MaterializeRetryRecoveryRun(args) => {
            let report = materialize_retry_recovery_run(*args)?;
            print_json(&report)?;
        }
        MeasurementCommand::RetryRecovery(args) => {
            let population: GeoPointPopulationArtifact = load_json(
                &args.population,
                CANON_GEO_POINT_POPULATION_VERSION,
                "population",
                "canon_geo_measurements measure-retry-recovery --population <POPULATION.json>",
            )?;
            let loops: Vec<GeoRetryLoopArtifact> = load_json_dir(
                &args.loops,
                CANON_GEO_RETRY_LOOP_VERSION,
                "retry loops",
                "loops",
                false,
            )?;
            let runs = load_geo_runs(&args.runs)?;
            let receipts = load_acquisition_receipts(&args.receipts)?;
            let artifact = measure_recovery(&population, &loops, &runs, &receipts)
                .map_err(|error| AppError::new(error.to_string()))?;
            let bytes = canonical_retry_recovery_bytes(&artifact).map_err(|error| {
                AppError::new(format!(
                    "failed to serialize {CANON_GEO_RETRY_RECOVERY_VERSION}: {error}"
                ))
            })?;
            write_canonical(&bytes)?;
        }
        MeasurementCommand::Population(args) => {
            let rows: GeoH7PopulationRowsRequest = load_json(
                &args.rows,
                CANON_GEO_H7_POPULATION_ROWS_VERSION,
                "rows",
                "canon_geo_measurements materialize-h7-population --rows <ROWS.json>",
            )?;
            let artifact = materialize_h7_population_rows(&rows)
                .map_err(|error| AppError::new(error.to_string()))?;
            let bytes = canonical_h7_population_bytes(&artifact).map_err(|error| {
                AppError::new(format!(
                    "failed to serialize {CANON_GEO_H7_POPULATION_VERSION}: {error}"
                ))
            })?;
            write_canonical(&bytes)?;
        }
        MeasurementCommand::StagingBatch(args) => {
            let batch: GeoH7StagingSourceRecordBytesBatchRequest = load_json(
                &args.batch,
                CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
                "batch",
                "canon_geo_measurements materialize-h7-staging-batch --batch <BATCH.json>",
            )?;
            let artifact = materialize_h7_staging_source_record_bytes_batch(&batch)
                .map_err(|error| AppError::new(error.to_string()))?;
            let bytes = canonical_h7_population_bytes(&artifact).map_err(|error| {
                AppError::new(format!(
                    "failed to serialize {CANON_GEO_H7_POPULATION_VERSION}: {error}"
                ))
            })?;
            write_canonical(&bytes)?;
        }
        MeasurementCommand::PipBlockBatch(args) => {
            let batch: GeoH7PipBlockPopulationBatchRequest = load_json(
                &args.batch,
                CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION,
                "batch",
                "canon_geo_measurements materialize-h7-pip-block-batch --batch <BATCH.json>",
            )?;
            let artifact = materialize_h7_pip_block_population_batch(&batch)
                .map_err(|error| AppError::new(error.to_string()))?;
            let bytes = canonical_h7_population_bytes(&artifact).map_err(|error| {
                AppError::new(format!(
                    "failed to serialize {CANON_GEO_H7_POPULATION_VERSION}: {error}"
                ))
            })?;
            write_canonical(&bytes)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn revalidate_d0_adjudication(args: RevalidateD0AdjudicationArgs) -> Result<ExitCode, AppError> {
    let labels_file: D0AdjudicationLabelsFile = load_unversioned_json(
        &args.labels,
        "D0 adjudication labels",
        "canon_geo_measurements revalidate-d0-adjudication --d0-labels <LABELS.json>",
    )?;
    if labels_file.version != D0_ADJUDICATION_LABELS_VERSION {
        return Err(AppError::new(format!(
            "unsupported D0 adjudication labels version {}; expected {D0_ADJUDICATION_LABELS_VERSION}",
            labels_file.version
        )));
    }
    let pins_file: D0AdjudicationPinsFile = load_unversioned_json(
        &args.pins,
        "D0 adjudication pins",
        "canon_geo_measurements revalidate-d0-adjudication --d0-pins <PINS.json>",
    )?;
    if pins_file.version != D0_ADJUDICATION_PINS_VERSION {
        return Err(AppError::new(format!(
            "unsupported D0 adjudication pins version {}; expected {D0_ADJUDICATION_PINS_VERSION}",
            pins_file.version
        )));
    }

    let labels = labels_file
        .labels
        .into_iter()
        .map(d0_retained_label_row)
        .collect::<Result<Vec<_>, _>>()?;
    let pins = d0_typed_tile_pins_by_id(pins_file.pins)?;
    let crop_bytes = d0_crop_bytes_by_case_id(args.crop_dir.as_deref(), &labels)?;
    let report = revalidate_adjudication_labels(&labels, &pins, &crop_bytes)
        .map_err(|error| AppError::new(error.to_string()))?;
    let has_invalid = !report.d0_labels_invalid.is_empty();
    print_json(&report)?;
    Ok(if has_invalid {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn d0_retained_label_row(
    row: D0AdjudicationLabelRow,
) -> Result<GeoAdjudicationRetainedLabelRow, AppError> {
    Ok(GeoAdjudicationRetainedLabelRow {
        case_id: row.case_id,
        subject_id: row.subject_id,
        pin_id: row.pin_id,
        window_blake3: row.window_blake3,
        candidate_parcel_ids: row.candidate_parcel_ids,
        overlay_geometry_blake3: row.overlay_geometry_blake3,
        crop_blake3: row.crop_blake3,
        label: d0_adjudication_label(row.label)?,
        adjudicator_id: row.adjudicator_id,
        truth_plane: row.truth_plane,
        notes_blake3: row.notes_blake3,
    })
}

fn d0_adjudication_label(label: D0AdjudicationLabel) -> Result<GeoAdjudicationLabel, AppError> {
    match label {
        D0AdjudicationLabel::Selected { selected_parcels } => {
            Ok(GeoAdjudicationLabel::SelectedParcels(selected_parcels))
        }
        D0AdjudicationLabel::Disposition(disposition) if disposition == "none_visible" => {
            Ok(GeoAdjudicationLabel::NoneVisible)
        }
        D0AdjudicationLabel::Disposition(disposition) if disposition == "unresolvable" => {
            Ok(GeoAdjudicationLabel::Unresolvable)
        }
        D0AdjudicationLabel::Disposition(disposition) => Err(AppError::new(format!(
            "unsupported D0 adjudication label disposition {disposition}"
        ))),
    }
}

fn d0_typed_tile_pins_by_id(
    pins: Vec<D0AdjudicationPin>,
) -> Result<BTreeMap<String, GeoImageTilePin>, AppError> {
    let mut by_id = BTreeMap::new();
    for pin in pins {
        let Some(blake3) = pin.blake3 else {
            continue;
        };
        let vintage = d0_vintage_interval(&pin.vintage)?;
        let pin_id = pin.pin_id;
        if by_id
            .insert(
                pin_id.clone(),
                GeoImageTilePin {
                    url: pin.url,
                    byte_range: pin.byte_range,
                    etag: pin.etag,
                    blake3,
                    vintage,
                    license_id: pin.license_id,
                    license_text_blake3: pin.license_text_blake3,
                    source_dataset: pin.source_dataset,
                },
            )
            .is_some()
        {
            return Err(AppError::new(format!(
                "duplicate typed D0 adjudication pin_id {pin_id}"
            )));
        }
    }
    Ok(by_id)
}

fn d0_crop_bytes_by_case_id(
    crop_dir: Option<&Path>,
    labels: &[GeoAdjudicationRetainedLabelRow],
) -> Result<BTreeMap<String, Vec<u8>>, AppError> {
    let Some(crop_dir) = crop_dir else {
        return Ok(BTreeMap::new());
    };
    let mut bytes_by_case = BTreeMap::new();
    for row in labels {
        let file_name = d0_crop_file_name(&row.case_id)?;
        let path = crop_dir.join(file_name);
        if !path.exists() {
            continue;
        }
        if bytes_by_case
            .insert(
                row.case_id.clone(),
                read_file(&path, "D0 adjudication crop bytes")?,
            )
            .is_some()
        {
            return Err(AppError::new(format!(
                "duplicate D0 adjudication crop case_id {}",
                row.case_id
            )));
        }
    }
    Ok(bytes_by_case)
}

fn d0_crop_file_name(case_id: &str) -> Result<String, AppError> {
    if case_id.is_empty()
        || !case_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AppError::new(format!(
            "D0 adjudication case_id cannot name a crop file safely: {case_id}"
        )));
    }
    Ok(format!("{case_id}.bin"))
}

fn d0_vintage_interval(vintage: &str) -> Result<GeoValidTimeInterval, AppError> {
    if vintage.len() != 4 || !vintage.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::new(format!(
            "D0 adjudication pin vintage must be a YYYY year, got {vintage}"
        )));
    }
    let year = vintage
        .parse::<i32>()
        .map_err(|error| AppError::new(format!("invalid D0 adjudication vintage: {error}")))?;
    Ok(GeoValidTimeInterval {
        start_day: epoch_day(year, 1, 1)?,
        end_day: epoch_day(year, 12, 31)?,
    })
}

fn epoch_day(year: i32, month: u32, day: u32) -> Result<i64, AppError> {
    let date = NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
        AppError::new(format!(
            "invalid D0 adjudication vintage date {year:04}-{month:02}-{day:02}"
        ))
    })?;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("Unix epoch date is valid");
    Ok(date.signed_duration_since(epoch).num_days())
}

fn materialize_acquisition_receipt(
    args: AcquisitionReceiptArgs,
) -> Result<GeoAcquisitionReceipt, AppError> {
    let request: canon::geo::GeoAcquisitionRequest = load_json(
        &args.request,
        CANON_GEO_ACQUISITION_REQUEST_VERSION,
        "request",
        "canon_geo_measurements materialize-acquisition-receipt --request <REQUEST.json>",
    )?;
    validate_geo_acquisition_request(&request)
        .map_err(|error| AppError::new(format!("invalid acquisition request: {error}")))?;

    let response_bytes = fs::read(&args.provider_response_bytes).map_err(|error| {
        AppError::new(format!(
            "failed to read provider response bytes {}: {error}",
            args.provider_response_bytes.display()
        ))
    })?;
    let candidate_rows_bytes = fs::read(&args.candidate_rows).map_err(|error| {
        AppError::new(format!(
            "failed to read candidate rows {}: {error}",
            args.candidate_rows.display()
        ))
    })?;
    let candidate_rows: Value = serde_json::from_slice(&candidate_rows_bytes).map_err(|error| {
        AppError::new(format!(
            "failed to parse candidate rows JSON {}: {error}",
            args.candidate_rows.display()
        ))
    })?;
    let row_count = candidate_row_count(&candidate_rows)?;
    let total_bytes = acquisition_byte_count(&response_bytes, &candidate_rows_bytes)?;
    let request_semantic_hash = geo_acquisition_request_semantic_hash(&request)
        .map_err(|error| AppError::new(format!("failed to hash acquisition request: {error}")))?;
    let canonical_request_bytes = canonical_geo_acquisition_request_bytes(&request)
        .map_err(|error| AppError::new(format!("failed to canonicalize request: {error}")))?;
    let response_digest = blake3_digest(PROVIDER_RESPONSE_BYTES_DIGEST_ID, &response_bytes);
    let candidate_rows_digest =
        blake3_digest(GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID, &candidate_rows_bytes);
    let proof_class = proof_class(args.proof_class, args.retained_receipt_id.as_ref())?;
    let retained_receipt_id = match proof_class {
        GeoAcquisitionProofClass::Retained => args.retained_receipt_id,
        GeoAcquisitionProofClass::Live => None,
        GeoAcquisitionProofClass::Fixture => unreachable!("fixture is not a command option"),
    };
    let receipt = GeoAcquisitionReceipt {
        version: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
        request_id: request.request_id.clone(),
        request_semantic_hash: request_semantic_hash.clone(),
        terminal_state: if row_count == 0 {
            GeoAcquisitionTerminalState::ZeroRows
        } else {
            GeoAcquisitionTerminalState::Complete
        },
        proof_class,
        executor: Some(GeoExecutorTrace {
            executor_kind: args.executor_kind.into(),
            executor_id: args.executor_id,
            executor_version: args.executor_version,
            tool_id: args.tool_id,
            tool_version: args.tool_version,
            executor_request_id: args.executor_request_id,
            executor_query_id: args.executor_query_id,
            executor_attempt_id: args.executor_attempt_id,
        }),
        fixture_id: None,
        retained_receipt_id,
        bounded_geography: request.bounded_geography.clone(),
        subset: request.subset.clone(),
        releases: request.releases.clone(),
        fields: request.fields.clone(),
        projection: request.projection.clone(),
        normalized_executed_request_digest: blake3_digest(
            "executor.normalized_request",
            &canonical_request_bytes,
        ),
        pagination: GeoPaginationReceipt {
            requested_page: request.pagination.clone(),
            next_page_token: None,
            rows_truncated: false,
            bytes_truncated: false,
        },
        counts: GeoAcquisitionCounts {
            rows: row_count,
            bytes: total_bytes,
        },
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "requested-subset".to_string(),
            source: GeoDenominatorSource::RequestedSubset,
            count: args
                .denominator_count
                .unwrap_or(request.positive_path_min_rows),
            unit: "row".to_string(),
            description: "bounded acquisition request subjects".to_string(),
        }],
        source_digests: source_release_digests(&request),
        result_digests: vec![response_digest.clone(), candidate_rows_digest.clone()],
        local_artifacts: vec![
            GeoLocalArtifactDigest {
                artifact_id: PROVIDER_RESPONSE_BYTES_DIGEST_ID.to_string(),
                media_type: "application/octet-stream".to_string(),
                byte_count: response_bytes.len() as u64,
                digest: response_digest,
            },
            GeoLocalArtifactDigest {
                artifact_id: GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID.to_string(),
                media_type: "application/json".to_string(),
                byte_count: candidate_rows_bytes.len() as u64,
                digest: candidate_rows_digest,
            },
        ],
        artifact_release_relations: Vec::new(),
        unreadable_columns: Vec::new(),
        resumability: GeoAcquisitionResumability {
            resumable: false,
            resume_token: None,
            resume_request_id: None,
            retry_guidance:
                "terminal acquisition receipt retained provider bytes and candidate rows"
                    .to_string(),
        },
        terminal_detail: None,
    };
    validate_geo_acquisition_receipt(&request, &receipt)
        .map_err(|error| AppError::new(format!("invalid acquisition receipt: {error}")))?;
    write_acquisition_receipt_sidecars(
        &args.out_dir,
        &request_semantic_hash,
        &response_bytes,
        &candidate_rows_bytes,
        &receipt,
    )?;
    Ok(receipt)
}

fn prepare_retry_recovery(
    args: PrepareRetryRecoveryArgs,
) -> Result<RetryRecoveryPreparationReport, AppError> {
    let population: GeoPointPopulationArtifact = load_json(
        &args.population,
        CANON_GEO_POINT_POPULATION_VERSION,
        "population",
        "canon_geo_measurements prepare-retry-recovery --population <POPULATION.json>",
    )?;
    validate_point_population_artifact(&population)
        .map_err(|error| AppError::new(format!("invalid point population: {error}")))?;
    if population.points.len() != G4_RETRY_RECOVERY_DENOMINATOR {
        return Err(AppError::new(format!(
            "prepare-retry-recovery requires the frozen {G4_RETRY_RECOVERY_DENOMINATOR}-point gross-class denominator, got {}",
            population.points.len()
        )));
    }
    if args.max_passes == 0 {
        return Err(AppError::new("--max-passes must be positive"));
    }

    let provider_profile_path = resolve_repo_relative(&args.repo_root, &args.provider_profile);
    let provider_profile_bytes = fs::read(&provider_profile_path).map_err(|error| {
        AppError::new(format!(
            "failed to read provider profile {}: {error}",
            provider_profile_path.display()
        ))
    })?;
    let provider_profile: Value =
        serde_json::from_slice(&provider_profile_bytes).map_err(|error| {
            AppError::new(format!(
                "failed to parse provider profile {}: {error}",
                provider_profile_path.display()
            ))
        })?;
    let provider_id = required_value_string(&provider_profile, "provider_id")?.to_string();
    let provider_version =
        required_value_string(&provider_profile, "provider_version")?.to_string();
    let provider_profile_digest = blake3_digest("provider_profile", &provider_profile_bytes);
    let address_rows = retry_address_rows(&args.address_rows)?;
    let request_dir = args.out_dir.join("requests");
    let loop_dir = args.out_dir.join("loops");
    let address_dir = args.out_dir.join("addresses");
    for dir in [&request_dir, &loop_dir, &address_dir] {
        fs::create_dir_all(dir).map_err(|error| {
            AppError::new(format!(
                "failed to create retry recovery output dir {}: {error}",
                dir.display()
            ))
        })?;
    }

    let mut request_semantic_hashes = BTreeMap::new();
    for point in &population.points {
        let address_row = address_rows.get(&point.point_id).ok_or_else(|| {
            AppError::new(format!(
                "address rows missing point_id {} for subject {}",
                point.point_id, point.subject_id
            ))
        })?;
        if address_row.subject_id != point.subject_id {
            return Err(AppError::new(format!(
                "address row subject mismatch for {}: expected {}, got {}",
                point.point_id, point.subject_id, address_row.subject_id
            )));
        }
        let address_hash = blake3::hash(address_row.asserted_address.trim().as_bytes())
            .to_hex()
            .to_string();
        if address_hash != point.asserted_address_blake3 {
            return Err(AppError::new(format!(
                "address row {} does not match asserted_address_blake3: expected {}, got {}",
                point.point_id, point.asserted_address_blake3, address_hash
            )));
        }
        let request = retry_recovery_request_for_point(
            point,
            &provider_id,
            &provider_version,
            provider_profile_digest.clone(),
            args.max_bytes,
        )?;
        let request_hash = geo_acquisition_request_semantic_hash(&request).map_err(|error| {
            AppError::new(format!(
                "failed to hash acquisition request for {}: {error}",
                point.point_id
            ))
        })?;
        let loop_state = GeoRetryLoopArtifact {
            version: CANON_GEO_RETRY_LOOP_VERSION.to_string(),
            subject_id: point.subject_id.clone(),
            policy: GeoRetryPolicy {
                max_passes: args.max_passes,
                regeocode_request_template: request.clone(),
            },
            passes: Vec::new(),
            terminal: None,
        };
        validate_retry_loop_artifact(&loop_state).map_err(|error| {
            AppError::new(format!(
                "failed to validate retry loop seed for {}: {error}",
                point.point_id
            ))
        })?;
        fs::write(
            request_dir.join(format!("{}.request.json", point.point_id)),
            serde_json::to_vec_pretty(&request).map_err(|error| {
                AppError::new(format!(
                    "failed to serialize acquisition request for {}: {error}",
                    point.point_id
                ))
            })?,
        )
        .map_err(|error| {
            AppError::new(format!(
                "failed to write acquisition request for {}: {error}",
                point.point_id
            ))
        })?;
        fs::write(
            loop_dir.join(format!("{}.loop.json", point.point_id)),
            canonical_retry_loop_bytes(&loop_state).map_err(|error| {
                AppError::new(format!(
                    "failed to serialize retry loop for {}: {error}",
                    point.point_id
                ))
            })?,
        )
        .map_err(|error| {
            AppError::new(format!(
                "failed to write retry loop seed for {}: {error}",
                point.point_id
            ))
        })?;
        fs::write(
            address_dir.join(format!("{}.address.txt", point.point_id)),
            format!("{}\n", address_row.one_line_address.trim()),
        )
        .map_err(|error| {
            AppError::new(format!(
                "failed to write retry recovery address for {}: {error}",
                point.point_id
            ))
        })?;
        request_semantic_hashes.insert(point.point_id.clone(), request_hash);
    }

    if address_rows.len() != population.points.len() {
        let population_point_ids = population
            .points
            .iter()
            .map(|point| point.point_id.as_str())
            .collect::<BTreeSet<_>>();
        let extra = address_rows
            .keys()
            .find(|point_id| !population_point_ids.contains(point_id.as_str()))
            .expect("length mismatch implies an extra address row");
        return Err(AppError::new(format!(
            "address rows contain point_id {extra} outside the frozen population"
        )));
    }

    Ok(RetryRecoveryPreparationReport {
        population_id: population.population_id,
        denominator: G4_RETRY_RECOVERY_DENOMINATOR as u64,
        provider_id,
        provider_version,
        request_dir: request_dir.display().to_string(),
        loop_dir: loop_dir.display().to_string(),
        address_dir: address_dir.display().to_string(),
        prepared: request_semantic_hashes.len() as u64,
        request_semantic_hashes,
    })
}

fn record_retry_recovery_pass(
    args: RecordRetryRecoveryPassArgs,
) -> Result<RetryRecoveryPassRecordReport, AppError> {
    fs::create_dir_all(&args.work_dir).map_err(|error| {
        AppError::new(format!(
            "failed to create retry-pass work dir {}: {error}",
            args.work_dir.display()
        ))
    })?;
    let loop_bytes = read_file(&args.retry_loop, "retry loop")?;
    let latest_run_bytes = read_file(&args.latest_run, "latest Geo run")?;
    let receipt_bytes = read_file(&args.receipt, "acquisition receipt")?;
    let loop_state: GeoRetryLoopArtifact = decode_versioned_json_bytes(
        &loop_bytes,
        CANON_GEO_RETRY_LOOP_VERSION,
        "retry loop",
        "canon_geo_measurements record-retry-recovery-pass --loop <LOOP.json>",
    )?;
    validate_retry_loop_artifact(&loop_state)
        .map_err(|error| AppError::new(format!("invalid retry loop: {error}")))?;
    let latest_run: GeoRun = decode_versioned_json_bytes(
        &latest_run_bytes,
        CANON_GEO_RUN_VERSION,
        "latest Geo run",
        "canon_geo_measurements record-retry-recovery-pass --latest-run <RUN.json>",
    )?;
    validate_geo_run(&latest_run)
        .map_err(|error| AppError::new(format!("invalid latest Geo run: {error}")))?;
    let _: GeoAcquisitionReceipt = decode_versioned_json_bytes(
        &receipt_bytes,
        CANON_GEO_ACQUISITION_RECEIPT_VERSION,
        "acquisition receipt",
        "canon_geo_measurements record-retry-recovery-pass --receipt <RECEIPT.json>",
    )?;
    let pass_index = next_retry_record_pass_index(&loop_state)?;
    let retry_loop_output_path = retry_recovery_pass_output_path(&loop_state, pass_index);
    let plan = retry_recovery_pass_plan(
        &loop_state,
        &loop_bytes,
        &latest_run_bytes,
        &receipt_bytes,
        &retry_loop_output_path,
    )?;
    let input_bindings = vec![
        GeoRunArtifactBinding::from_bytes(
            RETRY_RECOVERY_PASS_STAGE_NODE_ID,
            GEO_RETRY_LOOP_BINDING_ID,
            CANON_GEO_RETRY_LOOP_VERSION,
            loop_bytes,
        ),
        GeoRunArtifactBinding::from_bytes(
            RETRY_RECOVERY_PASS_STAGE_NODE_ID,
            GEO_RETRY_RUN_BINDING_ID,
            CANON_GEO_RUN_VERSION,
            latest_run_bytes,
        ),
        GeoRunArtifactBinding::from_bytes(
            RETRY_RECOVERY_PASS_STAGE_NODE_ID,
            GEO_RETRY_RECEIPT_BINDING_ID,
            CANON_GEO_ACQUISITION_RECEIPT_VERSION,
            receipt_bytes,
        ),
    ];
    let mut policy = ProjectRunPolicy::new(&args.work_dir, "work");
    policy.failure_policy = ProjectRunFailurePolicy::FailFast;
    let stage_plan_id = plan.plan_id.clone();
    let stage_plan_semantic_hash = plan.semantic_hash.clone();
    let stage_run = run_geo_plan(canon::geo::GeoRunRequest::new(plan, policy, input_bindings))
        .map_err(|error| AppError::new(format!("retry recovery pass stage failed: {error}")))?;
    if stage_run.status != GeoRunStatus::Completed {
        let reason = retry_pass_stage_failure_reason(&stage_run)
            .unwrap_or_else(|| "no failed-node reason was recorded".to_string());
        return Err(AppError::new(format!(
            "retry recovery pass stage did not complete: {:?}: {reason}",
            stage_run.status,
        )));
    }
    let stage_output_path = retry_loop_output_path_from_stage_run(&stage_run)?;
    let recorded_loop_bytes =
        fs::read(args.work_dir.join(&stage_output_path)).map_err(|error| {
            AppError::new(format!(
                "failed to read retry-pass stage output {}: {error}",
                stage_output_path
            ))
        })?;
    let recorded_loop: GeoRetryLoopArtifact = decode_versioned_json_bytes(
        &recorded_loop_bytes,
        CANON_GEO_RETRY_LOOP_VERSION,
        "recorded retry loop",
        "canon_geo_measurements record-retry-recovery-pass --out-loop <LOOP.json>",
    )?;
    validate_retry_loop_artifact(&recorded_loop)
        .map_err(|error| AppError::new(format!("invalid recorded retry loop: {error}")))?;
    let canonical_loop_bytes = canonical_retry_loop_bytes(&recorded_loop).map_err(|error| {
        AppError::new(format!(
            "failed to serialize recorded {CANON_GEO_RETRY_LOOP_VERSION}: {error}"
        ))
    })?;
    let canonical_run_bytes = canonical_geo_run_bytes(&stage_run).map_err(|error| {
        AppError::new(format!(
            "failed to serialize retry-pass {CANON_GEO_RUN_VERSION}: {error}"
        ))
    })?;
    write_bytes_file(&args.out_loop, &canonical_loop_bytes, "recorded retry loop")?;
    write_bytes_file(&args.out_run, &canonical_run_bytes, "retry-pass Geo run")?;
    Ok(RetryRecoveryPassRecordReport {
        subject_id: recorded_loop.subject_id,
        pass_index,
        terminal: recorded_loop.terminal,
        project_node_id: RETRY_RECOVERY_PASS_STAGE_NODE_ID.to_string(),
        stage_plan_id,
        stage_plan_semantic_hash,
        stage_run_id: stage_run.run_id,
        stage_run_semantic_hash: stage_run.semantic_hash,
        retry_loop_output_path: stage_output_path,
        out_loop: args.out_loop.display().to_string(),
        out_run: args.out_run.display().to_string(),
    })
}

fn materialize_retry_recovery_run(
    args: MaterializeRetryRecoveryRunArgs,
) -> Result<RetryRecoveryRunMaterializationReport, AppError> {
    fs::create_dir_all(&args.work_dir).map_err(|error| {
        AppError::new(format!(
            "failed to create retry-recovery run work dir {}: {error}",
            args.work_dir.display()
        ))
    })?;
    let population: GeoPointPopulationArtifact = load_json(
        &args.population,
        CANON_GEO_POINT_POPULATION_VERSION,
        "population",
        "canon_geo_measurements materialize-retry-recovery-run --population <POPULATION.json>",
    )?;
    validate_point_population_artifact(&population)
        .map_err(|error| AppError::new(format!("invalid point population: {error}")))?;
    if population.points.len() != G4_RETRY_RECOVERY_DENOMINATOR {
        return Err(AppError::new(format!(
            "materialize-retry-recovery-run requires the frozen {G4_RETRY_RECOVERY_DENOMINATOR}-point gross-class denominator, got {}",
            population.points.len()
        )));
    }
    let point = population
        .points
        .iter()
        .find(|point| point.point_id == args.point_id)
        .ok_or_else(|| {
            AppError::new(format!(
                "point_id {} is outside the frozen retry-recovery population",
                args.point_id
            ))
        })?;

    let loop_bytes = read_file(&args.retry_loop, "retry loop")?;
    let receipt_bytes = read_file(&args.receipt, "acquisition receipt")?;
    let candidate_rows_bytes = read_file(&args.candidate_rows, "candidate rows")?;
    let loop_state: GeoRetryLoopArtifact = decode_versioned_json_bytes(
        &loop_bytes,
        CANON_GEO_RETRY_LOOP_VERSION,
        "retry loop",
        "canon_geo_measurements materialize-retry-recovery-run --loop <LOOP.json>",
    )?;
    validate_retry_loop_artifact(&loop_state)
        .map_err(|error| AppError::new(format!("invalid retry loop: {error}")))?;
    if loop_state.subject_id != point.subject_id {
        return Err(AppError::new(format!(
            "retry loop subject mismatch for {}: expected {}, got {}",
            point.point_id, point.subject_id, loop_state.subject_id
        )));
    }
    let receipt: GeoAcquisitionReceipt = decode_versioned_json_bytes(
        &receipt_bytes,
        CANON_GEO_ACQUISITION_RECEIPT_VERSION,
        "acquisition receipt",
        "canon_geo_measurements materialize-retry-recovery-run --receipt <RECEIPT.json>",
    )?;
    if receipt.proof_class == GeoAcquisitionProofClass::Fixture {
        return Err(AppError::new(
            "materialize-retry-recovery-run requires retained or live acquisition receipts; fixture receipts cannot drive G4 recovery",
        ));
    }
    validate_retry_run_receipt_binding(
        point,
        &loop_state.policy.regeocode_request_template,
        &receipt,
        &candidate_rows_bytes,
    )?;
    let candidate_values = retry_recovery_candidate_row_values(&candidate_rows_bytes)?;
    let candidates = retry_recovery_candidates(point, &receipt, candidate_values)?;
    if candidates.is_empty() {
        return Err(AppError::new(format!(
            "receipt {} has zero candidate rows for {}; record a blocked retry pass with reason no_candidates instead of a home-cell run",
            receipt.request_semantic_hash, point.point_id
        )));
    }
    if receipt.counts.rows != candidates.len() as u64 {
        return Err(AppError::new(format!(
            "receipt {} row count mismatch: expected {}, parsed {}",
            receipt.request_semantic_hash,
            receipt.counts.rows,
            candidates.len()
        )));
    }

    let home_cell_rows = retry_recovery_home_cell_rows_request(point, &receipt, &candidates)?;
    let home_cell_rows_bytes = serde_json::to_vec(&home_cell_rows).map_err(|error| {
        AppError::new(format!(
            "failed to serialize {CANON_GEO_HOME_CELL_ROWS_VERSION}: {error}"
        ))
    })?;
    let home_cell_output_path = retry_recovery_home_cell_output_path(point);
    let plan = retry_recovery_home_cell_plan(
        point,
        &loop_bytes,
        &receipt_bytes,
        &candidate_rows_bytes,
        &home_cell_rows_bytes,
        &home_cell_output_path,
    )?;
    let stage_plan_id = plan.plan_id.clone();
    let stage_plan_semantic_hash = plan.semantic_hash.clone();
    let mut policy = ProjectRunPolicy::new(&args.work_dir, "work");
    policy.failure_policy = ProjectRunFailurePolicy::FailFast;
    let stage_run = run_geo_plan(canon::geo::GeoRunRequest::new(
        plan,
        policy,
        vec![GeoRunArtifactBinding::from_bytes(
            RETRY_RECOVERY_HOME_CELL_STAGE_NODE_ID,
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            home_cell_rows_bytes.clone(),
        )],
    ))
    .map_err(|error| AppError::new(format!("retry recovery home-cell stage failed: {error}")))?;
    if stage_run.status != GeoRunStatus::Completed {
        let reason = retry_home_cell_stage_failure_reason(&stage_run)
            .unwrap_or_else(|| "no failed-node reason was recorded".to_string());
        return Err(AppError::new(format!(
            "retry recovery home-cell stage did not complete: {:?}: {reason}",
            stage_run.status
        )));
    }
    let canonical_run_bytes = canonical_geo_run_bytes(&stage_run).map_err(|error| {
        AppError::new(format!(
            "failed to serialize home-cell {CANON_GEO_RUN_VERSION}: {error}"
        ))
    })?;
    write_bytes_file(&args.out_run, &canonical_run_bytes, "home-cell Geo run")?;
    if let Some(path) = &args.out_home_cell_rows {
        write_bytes_file(path, &home_cell_rows_bytes, "home-cell rows")?;
    }

    Ok(RetryRecoveryRunMaterializationReport {
        point_id: point.point_id.clone(),
        subject_id: point.subject_id.clone(),
        candidate_count: candidates.len() as u64,
        project_node_id: RETRY_RECOVERY_HOME_CELL_STAGE_NODE_ID.to_string(),
        stage_plan_id,
        stage_plan_semantic_hash,
        stage_run_id: stage_run.run_id.clone(),
        stage_run_semantic_hash: stage_run.semantic_hash.clone(),
        stage_run_status: stage_run.status,
        singleton_home_cell_r9: retry_recovery_singleton_home_cell(&stage_run),
        home_cell_rows_output_path: args
            .out_home_cell_rows
            .as_ref()
            .map(|path| path.display().to_string()),
        out_run: args.out_run.display().to_string(),
        precision_claim: false,
    })
}

fn validate_retry_run_receipt_binding(
    point: &GeoPointPopulationPoint,
    request: &canon::geo::GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
    candidate_rows_bytes: &[u8],
) -> Result<(), AppError> {
    let expected_request_hash =
        geo_acquisition_request_semantic_hash(request).map_err(|error| {
            AppError::new(format!(
                "failed to hash acquisition request for {}: {error}",
                point.point_id
            ))
        })?;
    if receipt.request_semantic_hash != expected_request_hash {
        return Err(AppError::new(format!(
            "receipt request_semantic_hash mismatch for {}: expected {}, got {}",
            point.point_id, expected_request_hash, receipt.request_semantic_hash
        )));
    }
    validate_geo_acquisition_receipt(request, receipt)
        .map_err(|error| AppError::new(format!("invalid acquisition receipt: {error}")))?;
    verify_candidate_rows_bytes_against_receipt(receipt, candidate_rows_bytes)?;
    Ok(())
}

fn verify_candidate_rows_bytes_against_receipt(
    receipt: &GeoAcquisitionReceipt,
    candidate_rows_bytes: &[u8],
) -> Result<(), AppError> {
    let actual = blake3::hash(candidate_rows_bytes).to_hex().to_string();
    let artifact = receipt
        .local_artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID)
        .ok_or_else(|| {
            AppError::new(format!(
                "receipt {} is missing local artifact {GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID}",
                receipt.request_semantic_hash
            ))
        })?;
    if artifact.digest.algorithm != GeoDigestAlgorithm::Blake3 {
        return Err(AppError::new(format!(
            "receipt {} local artifact {GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID} must use Blake3",
            receipt.request_semantic_hash
        )));
    }
    if artifact.byte_count != candidate_rows_bytes.len() as u64 {
        return Err(AppError::new(format!(
            "candidate rows byte count mismatch for {}: expected {}, actual {}",
            receipt.request_semantic_hash,
            artifact.byte_count,
            candidate_rows_bytes.len()
        )));
    }
    if artifact.digest.hex_digest != actual {
        return Err(AppError::new(format!(
            "candidate rows digest mismatch for {}: expected {}, actual {}",
            receipt.request_semantic_hash, artifact.digest.hex_digest, actual
        )));
    }
    let result_digest = receipt
        .result_digests
        .iter()
        .find(|digest| digest.digest_id == GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID)
        .ok_or_else(|| {
            AppError::new(format!(
                "receipt {} is missing result digest {GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID}",
                receipt.request_semantic_hash
            ))
        })?;
    if result_digest.algorithm != GeoDigestAlgorithm::Blake3
        || result_digest.hex_digest != artifact.digest.hex_digest
    {
        return Err(AppError::new(format!(
            "receipt {} result digest {GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID} is stale relative to the local artifact digest",
            receipt.request_semantic_hash
        )));
    }
    Ok(())
}

fn retry_recovery_candidate_row_values(
    candidate_rows_bytes: &[u8],
) -> Result<Vec<Value>, AppError> {
    let value: Value = serde_json::from_slice(candidate_rows_bytes)
        .map_err(|error| AppError::new(format!("failed to parse candidate rows JSON: {error}")))?;
    if let Some(rows) = value.as_array() {
        return Ok(rows.clone());
    }
    for field in ["rows", "candidates"] {
        if let Some(rows) = value.get(field).and_then(Value::as_array) {
            return Ok(rows.clone());
        }
    }
    Err(AppError::new(
        "candidate rows must be a JSON array or an object with rows[]/candidates[]",
    ))
}

fn retry_recovery_candidates(
    point: &GeoPointPopulationPoint,
    receipt: &GeoAcquisitionReceipt,
    values: Vec<Value>,
) -> Result<Vec<RetryRecoveryCandidate>, AppError> {
    let provider_id = receipt
        .executor
        .as_ref()
        .map(|executor| executor.executor_id.as_str());
    let provider_version = receipt
        .executor
        .as_ref()
        .map(|executor| executor.executor_version.as_str());
    let mut candidates = Vec::with_capacity(values.len());
    let mut ranks = BTreeSet::new();
    for value in values {
        let row: RetryRecoveryCandidateRow =
            serde_json::from_value(value.clone()).map_err(|error| {
                AppError::new(format!(
                    "failed to decode candidate row for {}: {error}",
                    point.point_id
                ))
            })?;
        if row.point_id != point.point_id {
            return Err(AppError::new(format!(
                "candidate row point_id mismatch: expected {}, got {}",
                point.point_id, row.point_id
            )));
        }
        if let Some(subject_id) = &row.subject_id
            && subject_id != &point.subject_id
        {
            return Err(AppError::new(format!(
                "candidate row subject_id mismatch for {}: expected {}, got {}",
                point.point_id, point.subject_id, subject_id
            )));
        }
        if row.candidate_rank == 0 {
            return Err(AppError::new(format!(
                "candidate row for {} must use one-based candidate_rank",
                point.point_id
            )));
        }
        if !ranks.insert(row.candidate_rank) {
            return Err(AppError::new(format!(
                "candidate rows for {} repeat candidate_rank {}",
                point.point_id, row.candidate_rank
            )));
        }
        if let (Some(actual), Some(expected)) = (row.provider_id.as_deref(), provider_id)
            && actual != expected
        {
            return Err(AppError::new(format!(
                "candidate row provider_id mismatch for {}: expected {}, got {}",
                point.point_id, expected, actual
            )));
        }
        if let (Some(actual), Some(expected)) = (row.provider_version.as_deref(), provider_version)
            && actual != expected
        {
            return Err(AppError::new(format!(
                "candidate row provider_version mismatch for {}: expected {}, got {}",
                point.point_id, expected, actual
            )));
        }
        candidates.push(RetryRecoveryCandidate { value, row });
    }
    candidates.sort_by_key(|candidate| candidate.row.candidate_rank);
    Ok(candidates)
}

fn retry_recovery_home_cell_rows_request(
    point: &GeoPointPopulationPoint,
    receipt: &GeoAcquisitionReceipt,
    candidates: &[RetryRecoveryCandidate],
) -> Result<GeoHomeCellRowsRequest, AppError> {
    let source = retry_recovery_candidate_source(receipt)?;
    let mut rows = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let row_bytes = serde_json::to_vec(&candidate.value).map_err(|error| {
            AppError::new(format!(
                "failed to serialize candidate row {} rank {}: {error}",
                point.point_id, candidate.row.candidate_rank
            ))
        })?;
        rows.push(GeoHomeCellRow {
            source: source.clone(),
            feature_id: format!(
                "geo.retry_recovery.{}.candidate.{:03}",
                point.point_id, candidate.row.candidate_rank
            ),
            source_record_id: format!(
                "geo.retry_recovery.{}.source_record.{:03}",
                point.point_id, candidate.row.candidate_rank
            ),
            geometry_sha256: sha256_hex(&row_bytes),
            representative_point_method: "provider_geocode_point_wgs84_e7".to_string(),
            longitude: fixed_e7_decimal(candidate.row.lon_e7),
            latitude: fixed_e7_decimal(candidate.row.lat_e7),
            transform_execution_id: None,
            transform_definition_id: None,
            claimed_home_cell: Some(point.home_cell_r9.clone()),
        });
    }
    Ok(GeoHomeCellRowsRequest {
        version: CANON_GEO_HOME_CELL_ROWS_VERSION.to_string(),
        coordinate_crs: "EPSG:4326".to_string(),
        coordinate_decimal_places: 7,
        h3_resolution: 9,
        stability_radius_fixed: 1,
        max_rows: rows.len() as u64,
        rows,
    })
}

fn retry_recovery_candidate_source(
    receipt: &GeoAcquisitionReceipt,
) -> Result<GeoTileSourceBinding, AppError> {
    if receipt.releases.len() != 1 {
        return Err(AppError::new(format!(
            "receipt {} must carry exactly one provider release pin, got {}",
            receipt.request_semantic_hash,
            receipt.releases.len()
        )));
    }
    let release = &receipt.releases[0];
    if release.release_digest.algorithm != GeoDigestAlgorithm::Blake3 {
        return Err(AppError::new(format!(
            "receipt {} provider release digest must use Blake3",
            receipt.request_semantic_hash
        )));
    }
    let executor = receipt.executor.as_ref().ok_or_else(|| {
        AppError::new(format!(
            "receipt {} requires executor trace for retry-recovery run materialization",
            receipt.request_semantic_hash
        ))
    })?;
    let source_digest = format!("blake3:{}", release.release_digest.hex_digest);
    let inventory_hash = digest_labeled_parts(
        "retry-recovery-home-cell-inventory",
        &[
            (
                "request_semantic_hash",
                receipt.request_semantic_hash.as_bytes(),
            ),
            ("release_id", release.release_id.as_bytes()),
            ("source_instance_id", release.source_instance_id.as_bytes()),
        ],
    );
    Ok(GeoTileSourceBinding {
        source_instance_id: executor.executor_id.clone(),
        release: GeoSourceRelease {
            release_id: release.release_id.clone(),
            release_digest: source_digest,
        },
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level: GeoControlEntityLevel::Address,
            identity_participation: GeoIdentityParticipation::EvidenceOnly,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: format!("geo.retry_recovery.acquisition.{}", executor.executor_id),
            semantic_hash: inventory_hash.clone(),
            planning_hash: inventory_hash,
        },
    })
}

fn retry_recovery_home_cell_plan(
    point: &GeoPointPopulationPoint,
    loop_bytes: &[u8],
    receipt_bytes: &[u8],
    candidate_rows_bytes: &[u8],
    home_cell_rows_bytes: &[u8],
    output_path: &str,
) -> Result<GeoPlan, AppError> {
    let point_digest = blake3::hash(point.point_id.as_bytes()).to_hex().to_string();
    let project_id = format!("geo.retry_recovery.home_cells.{point_digest}");
    let manifest_digest = digest_labeled_parts(
        "retry-recovery-home-cell-manifest",
        &[
            ("point_id", point.point_id.as_bytes()),
            ("subject_id", point.subject_id.as_bytes()),
            ("retry_loop", loop_bytes),
            ("receipt", receipt_bytes),
            ("candidate_rows", candidate_rows_bytes),
            ("home_cell_rows", home_cell_rows_bytes),
        ],
    );
    let lock_digest = digest_labeled_parts(
        "retry-recovery-home-cell-lock",
        &[
            ("project_id", project_id.as_bytes()),
            (
                "stage_command",
                GEO_MATERIALIZE_HOME_CELLS_COMMAND.as_bytes(),
            ),
            ("output_path", output_path.as_bytes()),
        ],
    );
    let project_plan =
        compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
            project_id,
            manifest_digest,
            lock_digest,
            vec![ProjectExtensionDagNode {
                node_id: RETRY_RECOVERY_HOME_CELL_STAGE_NODE_ID.to_string(),
                kind: ProjectPlanNodeKind::Normalize,
                class: ProjectPlanNodeClass::Computation,
                command: GEO_MATERIALIZE_HOME_CELLS_COMMAND.to_string(),
                dependencies: Vec::new(),
                content_hash_inputs: Vec::new(),
                outputs: vec![ProjectExtensionDagOutput {
                    output_id: "home_cells".to_string(),
                    path: output_path.to_string(),
                    materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
                }],
                limits: BTreeMap::new(),
                cache_eligible: true,
                side_effects: vec![
                    ProjectPlanSideEffect {
                        kind: ProjectPlanSideEffectKind::ReadsInput,
                        description:
                            "reads retained geocode candidate rows as typed home-cell rows"
                                .to_string(),
                    },
                    ProjectPlanSideEffect {
                        kind: ProjectPlanSideEffectKind::WritesArtifact,
                        description: "publishes one home-cell assignment for retry recovery"
                            .to_string(),
                    },
                ],
                refusal_conditions: vec![ProjectPlanRefusalCondition {
                    code: ProjectPlanErrorCode::ArtifactContract,
                    message: "refuse on candidate-row or home-cell assignment contract mismatch"
                        .to_string(),
                    next_command: None,
                }],
            }],
        ))
        .map_err(|error| AppError::new(format!("failed to compile home-cell DAG: {error}")))?;
    let question_hash = digest_labeled_parts(
        "retry-recovery-home-cell-question",
        &[
            ("point_id", point.point_id.as_bytes()),
            ("subject_id", point.subject_id.as_bytes()),
        ],
    );
    let capabilities_hash = digest_labeled_parts(
        "retry-recovery-home-cell-capabilities",
        &[(
            "stage_command",
            GEO_MATERIALIZE_HOME_CELLS_COMMAND.as_bytes(),
        )],
    );
    let inventory_hash = digest_labeled_parts(
        "retry-recovery-home-cell-inventory-ref",
        &[
            ("candidate_rows", candidate_rows_bytes),
            ("receipt", receipt_bytes),
        ],
    );
    let profile_hash = digest_labeled_parts(
        "retry-recovery-home-cell-profile",
        &[("selection_level", b"address")],
    );
    let budget_hash = digest_labeled_parts(
        "retry-recovery-home-cell-budget",
        &[("policy", b"local-artifact-only")],
    );
    let mut plan = GeoPlan {
        version: CANON_GEO_PLAN_VERSION.to_string(),
        plan_id: String::new(),
        semantic_hash: String::new(),
        status: GeoPlanStatus::Planned,
        question_ref: GeoPlanArtifactRef {
            artifact_id: "geo.retry_recovery.home_cell.question".to_string(),
            semantic_hash: question_hash,
        },
        capabilities_ref: GeoPlanArtifactRef {
            artifact_id: "geo.retry_recovery.home_cell.capabilities".to_string(),
            semantic_hash: capabilities_hash,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "geo.retry_recovery.home_cell.inventory".to_string(),
            semantic_hash: inventory_hash.clone(),
            planning_hash: inventory_hash,
        },
        profile_ref: GeoPlanProfileRef {
            version: "geo.retry_recovery.home_cell_profile.v0".to_string(),
            selection_level: GeoEntityLevel::Building,
            semantic_hash: profile_hash,
        },
        budget_ref: GeoPlanBudgetRef {
            budget_id: "geo.retry_recovery.home_cell.budget".to_string(),
            semantic_hash: budget_hash.clone(),
            planning_hash: budget_hash,
        },
        project_plan,
        geo_nodes: vec![GeoPlanNodeOverlay {
            project_node_id: RETRY_RECOVERY_HOME_CELL_STAGE_NODE_ID.to_string(),
            stage: GeoPlanStage::MaterializeHomeCells,
            entity_level: Some(GeoControlEntityLevel::Address),
            evidence_classes: vec![GeoEvidenceClass::GeocodePoint],
            claim_classes: vec![GeoClaimClass::CandidateReach],
            expected_output_contract: CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION.to_string(),
            preconditions: Vec::new(),
            claim_effect: GeoPlanClaimEffect::NamedAuditGate,
            bounded_section_required: false,
            incidence_factorization_required: false,
            exact_solve_scope: None,
            deterministic_bounds: Vec::new(),
            cost_estimate_ranges: Vec::new(),
            transitions: GeoPlanTransitionSet {
                success: "retained geocode candidate rows materialized as H3 home-cell candidates"
                    .to_string(),
                abstention: "multiple candidate rows remain non-singleton for retry scoring"
                    .to_string(),
                contradiction: "receipt and retained candidate rows disagree".to_string(),
                budget_fallback: "retry recovery home-cell stage has no internal fallback"
                    .to_string(),
            },
        }],
        grain_outcomes: vec![GeoPlanGrainOutcome {
            entity_level: GeoControlEntityLevel::Address,
            status: GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse,
            missing_evidence_classes: Vec::new(),
            project_node_ids: vec![RETRY_RECOVERY_HOME_CELL_STAGE_NODE_ID.to_string()],
            claim_limitation:
                "retry recovery home-cell materialization is reach-only; precision remains unclaimed"
                    .to_string(),
            next_action:
                "record this run into the retry loop, then measure recovered/abstained/blocked"
                    .to_string(),
        }],
        external_requests: Vec::new(),
        diagnostics: Vec::new(),
    };
    plan.semantic_hash = geo_plan_semantic_hash(&plan).map_err(|error| {
        AppError::new(format!(
            "failed to hash retry-recovery home-cell Geo plan: {error}"
        ))
    })?;
    plan.plan_id = format!(
        "{CANON_GEO_PLAN_VERSION}:{}",
        plan.semantic_hash.trim_start_matches("blake3:")
    );
    validate_geo_plan(&plan)
        .map_err(|error| AppError::new(format!("invalid home-cell Geo plan: {error}")))?;
    Ok(plan)
}

fn retry_recovery_home_cell_output_path(point: &GeoPointPopulationPoint) -> String {
    let point_digest = blake3::hash(point.point_id.as_bytes()).to_hex().to_string();
    format!("geo/retry_recovery/{point_digest}/home_cells.json")
}

fn retry_home_cell_stage_failure_reason(run: &GeoRun) -> Option<String> {
    run.project_run_report
        .as_ref()
        .and_then(|report| {
            report
                .node_reports
                .iter()
                .find(|node| {
                    node.node_id == RETRY_RECOVERY_HOME_CELL_STAGE_NODE_ID
                        && node.outcome == ProjectRunNodeOutcome::Failed
                })
                .and_then(|node| node.reason.clone())
        })
        .or_else(|| {
            run.blockers
                .iter()
                .find(|blocker| !blocker.reason.trim().is_empty())
                .map(|blocker| blocker.reason.clone())
        })
}

fn retry_recovery_singleton_home_cell(run: &GeoRun) -> Option<String> {
    run.output_refs
        .iter()
        .find(|output| {
            output.project_node_id == RETRY_RECOVERY_HOME_CELL_STAGE_NODE_ID
                && output.output_id == "home_cells"
        })
        .and_then(|output| output.home_cell_r9.clone())
}

fn fixed_e7_decimal(value: i64) -> String {
    let sign = if value.is_negative() { "-" } else { "" };
    let absolute = value.unsigned_abs();
    format!(
        "{sign}{}.{:07}",
        absolute / 10_000_000,
        absolute % 10_000_000
    )
}

fn retry_recovery_pass_plan(
    loop_state: &GeoRetryLoopArtifact,
    loop_bytes: &[u8],
    latest_run_bytes: &[u8],
    receipt_bytes: &[u8],
    output_path: &str,
) -> Result<GeoPlan, AppError> {
    let subject_digest = blake3::hash(loop_state.subject_id.as_bytes())
        .to_hex()
        .to_string();
    let project_id = format!("geo.retry_recovery.record.{subject_digest}");
    let manifest_digest = digest_labeled_parts(
        "retry-recovery-pass-manifest",
        &[
            ("retry_loop", loop_bytes),
            ("latest_run", latest_run_bytes),
            ("receipt", receipt_bytes),
        ],
    );
    let lock_digest = digest_labeled_parts(
        "retry-recovery-pass-lock",
        &[
            ("project_id", project_id.as_bytes()),
            ("stage_command", GEO_RETRY_PASS_STAGE_COMMAND.as_bytes()),
            ("output_path", output_path.as_bytes()),
        ],
    );
    let project_plan =
        compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
            project_id,
            manifest_digest,
            lock_digest,
            vec![ProjectExtensionDagNode {
                node_id: RETRY_RECOVERY_PASS_STAGE_NODE_ID.to_string(),
                kind: ProjectPlanNodeKind::Evidence,
                class: ProjectPlanNodeClass::Computation,
                command: GEO_RETRY_PASS_STAGE_COMMAND.to_string(),
                dependencies: Vec::new(),
                content_hash_inputs: Vec::new(),
                outputs: vec![ProjectExtensionDagOutput {
                    output_id: GEO_RETRY_LOOP_OUTPUT_ID.to_string(),
                    path: output_path.to_string(),
                    materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
                }],
                limits: BTreeMap::new(),
                cache_eligible: true,
                side_effects: vec![
                    ProjectPlanSideEffect {
                        kind: ProjectPlanSideEffectKind::ReadsInput,
                        description: "reads a typed retry loop, Geo run, and acquisition receipt"
                            .to_string(),
                    },
                    ProjectPlanSideEffect {
                        kind: ProjectPlanSideEffectKind::WritesArtifact,
                        description: "publishes one recorded retry-loop artifact".to_string(),
                    },
                ],
                refusal_conditions: vec![ProjectPlanRefusalCondition {
                    code: ProjectPlanErrorCode::ArtifactContract,
                    message: "refuse on retry loop, run, receipt, or output contract mismatch"
                        .to_string(),
                    next_command: None,
                }],
            }],
        ))
        .map_err(|error| AppError::new(format!("failed to compile retry-pass DAG: {error}")))?;
    let question_hash = digest_labeled_parts(
        "retry-recovery-pass-question",
        &[("subject_id", loop_state.subject_id.as_bytes())],
    );
    let capabilities_hash = digest_labeled_parts(
        "retry-recovery-pass-capabilities",
        &[("stage_command", GEO_RETRY_PASS_STAGE_COMMAND.as_bytes())],
    );
    let inventory_hash = digest_labeled_parts(
        "retry-recovery-pass-inventory",
        &[("retry_loop", loop_bytes), ("receipt", receipt_bytes)],
    );
    let profile_hash = digest_labeled_parts(
        "retry-recovery-pass-profile",
        &[("selection_level", b"building")],
    );
    let budget_hash = digest_labeled_parts(
        "retry-recovery-pass-budget",
        &[("policy", b"local-artifact-only")],
    );
    let mut plan = GeoPlan {
        version: CANON_GEO_PLAN_VERSION.to_string(),
        plan_id: String::new(),
        semantic_hash: String::new(),
        status: GeoPlanStatus::Planned,
        question_ref: GeoPlanArtifactRef {
            artifact_id: "geo.retry_recovery.record.question".to_string(),
            semantic_hash: question_hash,
        },
        capabilities_ref: GeoPlanArtifactRef {
            artifact_id: "geo.retry_recovery.record.capabilities".to_string(),
            semantic_hash: capabilities_hash,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "geo.retry_recovery.record.inventory".to_string(),
            semantic_hash: inventory_hash.clone(),
            planning_hash: inventory_hash,
        },
        profile_ref: GeoPlanProfileRef {
            version: "geo.retry_recovery.stage_profile.v0".to_string(),
            selection_level: GeoEntityLevel::Building,
            semantic_hash: profile_hash,
        },
        budget_ref: GeoPlanBudgetRef {
            budget_id: "geo.retry_recovery.record.budget".to_string(),
            semantic_hash: budget_hash.clone(),
            planning_hash: budget_hash,
        },
        project_plan,
        geo_nodes: vec![GeoPlanNodeOverlay {
            project_node_id: RETRY_RECOVERY_PASS_STAGE_NODE_ID.to_string(),
            stage: GeoPlanStage::MaterializeEvidence,
            entity_level: Some(GeoControlEntityLevel::Building),
            evidence_classes: vec![GeoEvidenceClass::GeocodePoint],
            claim_classes: vec![GeoClaimClass::CandidateReach],
            expected_output_contract: CANON_GEO_RETRY_LOOP_VERSION.to_string(),
            preconditions: Vec::new(),
            claim_effect: GeoPlanClaimEffect::NamedAuditGate,
            bounded_section_required: false,
            incidence_factorization_required: false,
            exact_solve_scope: None,
            deterministic_bounds: Vec::new(),
            cost_estimate_ranges: Vec::new(),
            transitions: GeoPlanTransitionSet {
                success: "recorded retry pass can enter retry-recovery measurement".to_string(),
                abstention: "retry loop remains bounded by policy ceiling".to_string(),
                contradiction: "receipt/run mismatch refuses before recovery scoring".to_string(),
                budget_fallback: "retry pass has no internal budget fallback".to_string(),
            },
        }],
        grain_outcomes: vec![GeoPlanGrainOutcome {
            entity_level: GeoControlEntityLevel::Building,
            status: GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse,
            missing_evidence_classes: Vec::new(),
            project_node_ids: vec![RETRY_RECOVERY_PASS_STAGE_NODE_ID.to_string()],
            claim_limitation:
                "retry-pass recording binds fresh acquisition receipts for reach only; precision remains unclaimed"
                    .to_string(),
            next_action:
                "after all frozen 40 loops are terminal, run measure-retry-recovery".to_string(),
        }],
        external_requests: Vec::new(),
        diagnostics: Vec::new(),
    };
    plan.semantic_hash = geo_plan_semantic_hash(&plan)
        .map_err(|error| AppError::new(format!("failed to hash retry-pass Geo plan: {error}")))?;
    plan.plan_id = format!(
        "{CANON_GEO_PLAN_VERSION}:{}",
        plan.semantic_hash.trim_start_matches("blake3:")
    );
    validate_geo_plan(&plan)
        .map_err(|error| AppError::new(format!("invalid retry-pass Geo plan: {error}")))?;
    Ok(plan)
}

fn next_retry_record_pass_index(loop_state: &GeoRetryLoopArtifact) -> Result<u8, AppError> {
    let next = loop_state
        .passes
        .len()
        .checked_add(1)
        .ok_or_else(|| AppError::new("retry loop pass count overflowed"))?;
    u8::try_from(next).map_err(|_| AppError::new("retry loop pass count exceeded u8 range"))
}

fn retry_recovery_pass_output_path(loop_state: &GeoRetryLoopArtifact, pass_index: u8) -> String {
    let subject_digest = blake3::hash(loop_state.subject_id.as_bytes())
        .to_hex()
        .to_string();
    format!("geo/retry_recovery/{subject_digest}/pass-{pass_index:02}/retry_loop.json")
}

fn retry_loop_output_path_from_stage_run(run: &GeoRun) -> Result<String, AppError> {
    let report = run
        .project_run_report
        .as_ref()
        .ok_or_else(|| AppError::new("retry-pass Geo run is missing project_run_report"))?;
    let node_receipt = report
        .receipt
        .node_receipts
        .iter()
        .find(|receipt| {
            receipt.node_id == RETRY_RECOVERY_PASS_STAGE_NODE_ID
                && receipt.outcome == ProjectRunNodeOutcome::Completed
        })
        .ok_or_else(|| AppError::new("retry-pass Geo run is missing a completed node receipt"))?;
    let output = node_receipt
        .outputs
        .iter()
        .find(|output| output.output_id == GEO_RETRY_LOOP_OUTPUT_ID)
        .ok_or_else(|| AppError::new("retry-pass node receipt is missing retry_loop output"))?;
    Ok(output.path.clone())
}

fn retry_pass_stage_failure_reason(run: &GeoRun) -> Option<String> {
    run.project_run_report
        .as_ref()
        .and_then(|report| {
            report
                .node_reports
                .iter()
                .find(|node| {
                    node.node_id == RETRY_RECOVERY_PASS_STAGE_NODE_ID
                        && node.outcome == ProjectRunNodeOutcome::Failed
                })
                .and_then(|node| node.reason.clone())
        })
        .or_else(|| {
            run.blockers
                .iter()
                .find(|blocker| !blocker.reason.trim().is_empty())
                .map(|blocker| blocker.reason.clone())
        })
}

fn retry_address_rows(path: &Path) -> Result<BTreeMap<String, RetryRecoveryAddressRow>, AppError> {
    let rows: Vec<RetryRecoveryAddressRow> = load_unversioned_json(
        path,
        "retry-recovery address rows",
        "canon_geo_measurements prepare-retry-recovery --address-rows <ROWS.json>",
    )?;
    let mut by_point_id = BTreeMap::new();
    for row in rows {
        if row.point_id.trim().is_empty() {
            return Err(AppError::new(
                "retry-recovery address rows require nonempty point_id",
            ));
        }
        if row.subject_id.trim().is_empty() {
            return Err(AppError::new(format!(
                "retry-recovery address row {} requires nonempty subject_id/property_key",
                row.point_id
            )));
        }
        if row.asserted_address.trim().is_empty() {
            return Err(AppError::new(format!(
                "retry-recovery address row {} requires nonempty asserted_address",
                row.point_id
            )));
        }
        if row.one_line_address.trim().is_empty() {
            return Err(AppError::new(format!(
                "retry-recovery address row {} requires nonempty one_line_address",
                row.point_id
            )));
        }
        if by_point_id.insert(row.point_id.clone(), row).is_some() {
            return Err(AppError::new(
                "retry-recovery address rows must be unique by point_id",
            ));
        }
    }
    Ok(by_point_id)
}

fn retry_recovery_request_for_point(
    point: &GeoPointPopulationPoint,
    provider_id: &str,
    provider_version: &str,
    provider_profile_digest: GeoDigest,
    max_bytes: u64,
) -> Result<canon::geo::GeoAcquisitionRequest, AppError> {
    let geography = GeoBoundedGeography {
        geography_id: format!("geo.retry-recovery.{}.geography", point.point_id),
        geography_kind: "gross_class_retry_recovery_point".to_string(),
        description: "frozen G4 gross-class point requiring external re-geocode acquisition"
            .to_string(),
    };
    let subset = GeoBoundedSubset {
        subset_id: format!("geo.retry-recovery.{}.subset", point.point_id),
        geography: geography.clone(),
        h3_cells: vec![point.home_cell_r9.clone()],
        predicates: vec![
            GeoSubsetPredicate {
                predicate_id: "point_id".to_string(),
                kind: GeoSubsetPredicateKind::ExplicitIdentifiers,
                expression: format!("point_id = '{}'", point.point_id),
            },
            GeoSubsetPredicate {
                predicate_id: "subject_id".to_string(),
                kind: GeoSubsetPredicateKind::ExplicitIdentifiers,
                expression: format!("subject_id = '{}'", point.subject_id),
            },
            GeoSubsetPredicate {
                predicate_id: "asserted_address_blake3".to_string(),
                kind: GeoSubsetPredicateKind::ExplicitIdentifiers,
                expression: format!(
                    "asserted_address_blake3 = '{}'",
                    point.asserted_address_blake3
                ),
            },
        ],
    };
    let mut request = canon::geo::GeoAcquisitionRequest {
        version: CANON_GEO_ACQUISITION_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        discovery_request_id: None,
        bounded_geography: geography,
        subset,
        releases: vec![GeoReleasePin {
            source_instance_id: provider_id.to_string(),
            release_id: provider_version.to_string(),
            release_digest: provider_profile_digest,
        }],
        fields: vec![
            GeoRequestedField {
                field_id: "point_id".to_string(),
                role: GeoFieldRole::Identifier,
                required: true,
            },
            GeoRequestedField {
                field_id: "subject_id".to_string(),
                role: GeoFieldRole::Identifier,
                required: true,
            },
            GeoRequestedField {
                field_id: "address_text".to_string(),
                role: GeoFieldRole::Identifier,
                required: true,
            },
            GeoRequestedField {
                field_id: "asserted_address_blake3".to_string(),
                role: GeoFieldRole::Digest,
                required: true,
            },
        ],
        projection: None,
        ordering: vec![GeoOrderingTerm {
            position: 1,
            field_id: "point_id".to_string(),
            direction: GeoOrderDirection::Asc,
            nulls: GeoNullOrdering::Last,
        }],
        pagination: GeoPaginationRequest {
            page_size_rows: 10,
            page_token: None,
        },
        ceilings: GeoRowByteCeilings {
            max_rows: 10,
            max_bytes,
        },
        positive_path_min_rows: 1,
    };
    request.request_id = geo_acquisition_request_id(&request).map_err(|error| {
        AppError::new(format!(
            "failed to derive acquisition request id for {}: {error}",
            point.point_id
        ))
    })?;
    validate_geo_acquisition_request(&request).map_err(|error| {
        AppError::new(format!(
            "invalid acquisition request for {}: {error}",
            point.point_id
        ))
    })?;
    Ok(request)
}

fn candidate_row_count(candidate_rows: &Value) -> Result<u64, AppError> {
    if let Some(rows) = candidate_rows.as_array() {
        return Ok(rows.len() as u64);
    }
    for field in ["rows", "candidates"] {
        if let Some(rows) = candidate_rows.get(field).and_then(Value::as_array) {
            return Ok(rows.len() as u64);
        }
    }
    Err(AppError::new(
        "candidate rows must be a JSON array or an object with rows[]/candidates[]",
    ))
}

fn acquisition_byte_count(
    response_bytes: &[u8],
    candidate_rows_bytes: &[u8],
) -> Result<u64, AppError> {
    (response_bytes.len() as u64)
        .checked_add(candidate_rows_bytes.len() as u64)
        .ok_or_else(|| AppError::new("acquisition receipt byte count overflowed"))
}

fn proof_class(
    proof: AcquisitionReceiptProofArg,
    retained_receipt_id: Option<&String>,
) -> Result<GeoAcquisitionProofClass, AppError> {
    match proof {
        AcquisitionReceiptProofArg::Retained => {
            if retained_receipt_id
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(AppError::new(
                    "retained proof requires --retained-receipt-id",
                ));
            }
            Ok(GeoAcquisitionProofClass::Retained)
        }
        AcquisitionReceiptProofArg::Live => {
            if retained_receipt_id.is_some() {
                return Err(AppError::new(
                    "live proof must not carry --retained-receipt-id",
                ));
            }
            Ok(GeoAcquisitionProofClass::Live)
        }
    }
}

fn source_release_digests(request: &canon::geo::GeoAcquisitionRequest) -> Vec<GeoDigest> {
    request
        .releases
        .iter()
        .map(|release| GeoDigest {
            digest_id: format!(
                "source_release:{}:{}:{}",
                release.source_instance_id, release.release_id, release.release_digest.hex_digest
            ),
            algorithm: release.release_digest.algorithm,
            hex_digest: release.release_digest.hex_digest.clone(),
        })
        .collect()
}

fn blake3_digest(digest_id: &str, bytes: &[u8]) -> GeoDigest {
    GeoDigest {
        digest_id: digest_id.to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: blake3::hash(bytes).to_hex().to_string(),
    }
}

fn write_acquisition_receipt_sidecars(
    out_dir: &Path,
    request_semantic_hash: &str,
    response_bytes: &[u8],
    candidate_rows_bytes: &[u8],
    receipt: &GeoAcquisitionReceipt,
) -> Result<(), AppError> {
    fs::create_dir_all(out_dir).map_err(|error| {
        AppError::new(format!(
            "failed to create acquisition receipt dir {}: {error}",
            out_dir.display()
        ))
    })?;
    fs::write(
        out_dir.join(format!("{request_semantic_hash}.bytes")),
        response_bytes,
    )
    .map_err(|error| AppError::new(format!("failed to write response sidecar: {error}")))?;
    fs::write(
        out_dir.join(format!("{request_semantic_hash}.rows.json")),
        candidate_rows_bytes,
    )
    .map_err(|error| AppError::new(format!("failed to write candidate rows sidecar: {error}")))?;
    let receipt_path = out_dir.join(format!(
        "{}.receipt.json",
        request_semantic_hash.replace(':', "_")
    ));
    let receipt_bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|error| AppError::new(format!("failed to serialize receipt: {error}")))?;
    fs::write(&receipt_path, receipt_bytes).map_err(|error| {
        AppError::new(format!(
            "failed to write receipt {}: {error}",
            receipt_path.display()
        ))
    })
}

impl From<AcquisitionExecutorKindArg> for GeoExecutorKind {
    fn from(kind: AcquisitionExecutorKindArg) -> Self {
        match kind {
            AcquisitionExecutorKindArg::Catalog => GeoExecutorKind::Catalog,
            AcquisitionExecutorKindArg::QueryEngine => GeoExecutorKind::QueryEngine,
            AcquisitionExecutorKindArg::ObjectStore => GeoExecutorKind::ObjectStore,
            AcquisitionExecutorKindArg::HttpService => GeoExecutorKind::HttpService,
            AcquisitionExecutorKindArg::LocalFile => GeoExecutorKind::LocalFile,
            AcquisitionExecutorKindArg::ManualExport => GeoExecutorKind::ManualExport,
            AcquisitionExecutorKindArg::Other => GeoExecutorKind::Other,
        }
    }
}

fn load_unversioned_json<T: DeserializeOwned>(
    path: &Path,
    label: &str,
    usage: &str,
) -> Result<T, AppError> {
    let bytes = fs::read(path)
        .map_err(|error| AppError::new(format!("failed to read {}: {error}", path.display())))?;
    serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(format!(
            "failed to decode {} as {label} for {usage}: {error}",
            path.display()
        ))
    })
}

fn read_file(path: &Path, label: &str) -> Result<Vec<u8>, AppError> {
    fs::read(path).map_err(|error| {
        AppError::new(format!(
            "failed to read {label} {}: {error}",
            path.display()
        ))
    })
}

fn decode_versioned_json_bytes<T: DeserializeOwned>(
    bytes: &[u8],
    expected_version: &str,
    label: &str,
    usage: &str,
) -> Result<T, AppError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        AppError::new(format!("failed to parse {label} JSON for {usage}: {error}"))
    })?;
    match value.get("version").and_then(Value::as_str) {
        Some(actual) if actual == expected_version => {}
        Some(actual) => {
            return Err(AppError::new(format!(
                "unsupported {label} version {actual}; expected {expected_version}"
            )));
        }
        None => {
            return Err(AppError::new(format!(
                "{usage} requires top-level version {expected_version}"
            )));
        }
    }
    serde_json::from_value(value).map_err(|error| {
        AppError::new(format!(
            "failed to decode {label} as {expected_version}: {error}"
        ))
    })
}

fn digest_labeled_parts(label: &str, parts: &[(&str, &[u8])]) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(label.as_bytes());
    bytes.push(b'\n');
    for (part_label, part_bytes) in parts {
        bytes.extend_from_slice(part_label.as_bytes());
        bytes.push(b'\0');
        bytes.extend_from_slice(part_bytes.len().to_string().as_bytes());
        bytes.push(b'\0');
        bytes.extend_from_slice(part_bytes);
        bytes.push(b'\n');
    }
    project_digest_bytes(&bytes)
}

fn write_bytes_file(path: &Path, bytes: &[u8], label: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::new(format!(
                "failed to create parent dir for {label} {}: {error}",
                path.display()
            ))
        })?;
    }
    fs::write(path, bytes).map_err(|error| {
        AppError::new(format!(
            "failed to write {label} {}: {error}",
            path.display()
        ))
    })
}

fn required_value_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, AppError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| AppError::new(format!("provider profile is missing nonempty {field}")))
}

fn load_json<T: DeserializeOwned>(
    path: &Path,
    expected_version: &str,
    version_field: &str,
    usage: &str,
) -> Result<T, AppError> {
    let bytes = fs::read(path)
        .map_err(|error| AppError::new(format!("failed to read {}: {error}", path.display())))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(format!(
            "failed to parse JSON {} for {usage}: {error}",
            path.display()
        ))
    })?;
    match value.get("version").and_then(Value::as_str) {
        Some(actual) if actual == expected_version => {}
        Some(actual) => {
            return Err(AppError::new(format!(
                "unsupported {version_field} version {actual}; expected {expected_version}"
            )));
        }
        None => {
            return Err(AppError::new(format!(
                "{usage} requires top-level version {expected_version}"
            )));
        }
    }
    serde_json::from_value(value).map_err(|error| {
        AppError::new(format!(
            "failed to decode {} as {expected_version}: {error}",
            path.display()
        ))
    })
}

fn load_json_dir<T: DeserializeOwned>(
    dir: &Path,
    expected_version: &str,
    label: &str,
    arg_name: &str,
    skip_candidate_rows: bool,
) -> Result<Vec<T>, AppError> {
    let files = sorted_json_files(dir, label, skip_candidate_rows)?;
    if files.is_empty() {
        return Err(AppError::new(format!(
            "{label} directory {} contains no JSON files",
            dir.display()
        )));
    }
    files
        .iter()
        .map(|path| {
            load_json(
                path,
                expected_version,
                label,
                &format!("canon_geo_measurements measure-retry-recovery --{arg_name} <DIR>"),
            )
        })
        .collect()
}

fn load_geo_runs(dir: &Path) -> Result<BTreeMap<String, GeoRun>, AppError> {
    let runs: Vec<GeoRun> = load_json_dir(dir, CANON_GEO_RUN_VERSION, "runs", "runs", false)?;
    let mut by_hash = BTreeMap::new();
    for run in runs {
        if by_hash.insert(run.semantic_hash.clone(), run).is_some() {
            return Err(AppError::new(
                "duplicate GeoRun semantic_hash in runs directory",
            ));
        }
    }
    Ok(by_hash)
}

fn load_acquisition_receipts(
    dir: &Path,
) -> Result<BTreeMap<String, GeoAcquisitionReceipt>, AppError> {
    let files = sorted_json_files(dir, "receipts", true)?;
    if files.is_empty() {
        return Err(AppError::new(format!(
            "receipts directory {} contains no receipt JSON files",
            dir.display()
        )));
    }
    let mut by_request_hash = BTreeMap::new();
    for path in files {
        let receipt: GeoAcquisitionReceipt = load_json(
            &path,
            CANON_GEO_ACQUISITION_RECEIPT_VERSION,
            "receipts",
            "canon_geo_measurements measure-retry-recovery --receipts <DIR>",
        )?;
        verify_retry_recovery_receipt_sidecars(dir, &receipt)?;
        if by_request_hash
            .insert(receipt.request_semantic_hash.clone(), receipt)
            .is_some()
        {
            return Err(AppError::new(
                "duplicate GeoAcquisitionReceipt request_semantic_hash in receipts directory",
            ));
        }
    }
    Ok(by_request_hash)
}

fn sorted_json_files(
    dir: &Path,
    label: &str,
    skip_candidate_rows: bool,
) -> Result<Vec<PathBuf>, AppError> {
    let entries = fs::read_dir(dir)
        .map_err(|error| AppError::new(format!("failed to read {label} dir: {error}")))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| AppError::new(format!("failed to read {label} dir entry: {error}")))?;
        let path = entry.path();
        let is_json = path.extension().and_then(|extension| extension.to_str()) == Some("json");
        let is_candidate_rows = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".rows.json"));
        let is_receipt_stdout_copy = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".receipt.stdout.json"));
        if is_json && !(skip_candidate_rows && (is_candidate_rows || is_receipt_stdout_copy)) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn verify_retry_recovery_receipt_sidecars(
    dir: &Path,
    receipt: &GeoAcquisitionReceipt,
) -> Result<(), AppError> {
    let response_digest = receipt
        .result_digests
        .iter()
        .find(|digest| digest.digest_id == PROVIDER_RESPONSE_BYTES_DIGEST_ID)
        .ok_or_else(|| {
            AppError::new(format!(
                "receipt {} is missing result digest {PROVIDER_RESPONSE_BYTES_DIGEST_ID}",
                receipt.request_semantic_hash
            ))
        })?;
    if response_digest.algorithm != GeoDigestAlgorithm::Blake3 {
        return Err(AppError::new(format!(
            "receipt {} result digest {PROVIDER_RESPONSE_BYTES_DIGEST_ID} must use Blake3",
            receipt.request_semantic_hash
        )));
    }
    let response_artifact = receipt
        .local_artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == PROVIDER_RESPONSE_BYTES_DIGEST_ID)
        .ok_or_else(|| {
            AppError::new(format!(
                "receipt {} is missing local artifact {PROVIDER_RESPONSE_BYTES_DIGEST_ID}",
                receipt.request_semantic_hash
            ))
        })?;
    if response_artifact.digest.algorithm != GeoDigestAlgorithm::Blake3
        || response_artifact.digest.hex_digest != response_digest.hex_digest
    {
        return Err(AppError::new(format!(
            "receipt {} has stale {PROVIDER_RESPONSE_BYTES_DIGEST_ID} local artifact digest",
            receipt.request_semantic_hash
        )));
    }
    verify_blake3_sidecar(
        &dir.join(format!("{}.bytes", receipt.request_semantic_hash)),
        &response_digest.hex_digest,
        PROVIDER_RESPONSE_BYTES_DIGEST_ID,
    )?;

    let candidate_rows_artifact = receipt
        .local_artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID)
        .ok_or_else(|| {
            AppError::new(format!(
                "receipt {} is missing local artifact {GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID}",
                receipt.request_semantic_hash
            ))
        })?;
    if candidate_rows_artifact.digest.algorithm != GeoDigestAlgorithm::Blake3 {
        return Err(AppError::new(format!(
            "receipt {} local artifact {GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID} must use Blake3",
            receipt.request_semantic_hash
        )));
    }
    verify_blake3_sidecar(
        &dir.join(format!("{}.rows.json", receipt.request_semantic_hash)),
        &candidate_rows_artifact.digest.hex_digest,
        GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID,
    )
}

fn verify_blake3_sidecar(path: &Path, expected: &str, digest_id: &str) -> Result<(), AppError> {
    let bytes = fs::read(path).map_err(|error| {
        AppError::new(format!(
            "failed to read {digest_id} sidecar {}: {error}",
            path.display()
        ))
    })?;
    let actual = blake3::hash(&bytes).to_hex().to_string();
    if actual != expected {
        return Err(AppError::new(format!(
            "sidecar {} digest mismatch for {digest_id}: expected {expected}, actual {actual}",
            path.display()
        )));
    }
    Ok(())
}

fn load_manifest(repo_root: &Path, manifest_path: &Path) -> Result<Manifest, AppError> {
    let manifest_path = resolve_repo_relative(repo_root, manifest_path);
    let bytes = fs::read(&manifest_path).map_err(|error| {
        AppError::new(format!(
            "failed to read manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(format!(
            "failed to parse manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    validate_manifest(repo_root, manifest)
}

fn validate_manifest(repo_root: &Path, manifest: Manifest) -> Result<Manifest, AppError> {
    if manifest.version != MANIFEST_VERSION {
        return Err(AppError::new(format!(
            "unsupported manifest version {}; expected {MANIFEST_VERSION}",
            manifest.version
        )));
    }
    if !manifest.offline_only {
        return Err(AppError::new(
            "manifest must be offline_only; this runner never executes Snowflake or warehouse queries",
        ));
    }
    validate_required_measurement_ids(&manifest.required_measurement_ids)?;
    let declared = manifest.required_measurement_ids.clone();
    let measurement_ids = manifest
        .measurements
        .iter()
        .map(|measurement| measurement.id.clone())
        .collect::<Vec<_>>();
    if measurement_ids != declared {
        return Err(AppError::new(
            "manifest measurements must appear exactly in required_measurement_ids order",
        ));
    }

    let mut seen = BTreeSet::new();
    let declared_set = declared.iter().cloned().collect::<BTreeSet<_>>();
    for measurement in &manifest.measurements {
        if !seen.insert(measurement.id.clone()) {
            return Err(AppError::new(format!(
                "duplicate manifest measurement id {}",
                measurement.id
            )));
        }
        if !declared_set.contains(&measurement.id) {
            return Err(AppError::new(format!(
                "manifest contains undeclared measurement id {}",
                measurement.id
            )));
        }
        if measurement.id.to_ascii_lowercase().contains("h7")
            || measurement.section.to_ascii_lowercase().starts_with('h')
        {
            return Err(AppError::new(format!(
                "manifest measurement {} is outside bd-3mo1 scope; H7 is excluded",
                measurement.id
            )));
        }
        validate_gate(&measurement.id, &measurement.gate)?;
        for (field, value) in [
            ("geography", &measurement.geography),
            ("tier", &measurement.tier),
            ("declared_grain", &measurement.declared_grain),
        ] {
            if value.trim().is_empty() {
                return Err(AppError::new(format!(
                    "manifest measurement {} {field} must be nonempty",
                    measurement.id
                )));
            }
        }
        validate_measurement_claim_boundary(measurement)?;
        validate_relative_sql_path(&measurement.sql_path)?;
        validate_sha256("source_sql_sha256", &measurement.source_sql_sha256)?;
        if measurement.execution_transform != EXECUTION_TRANSFORM {
            return Err(AppError::new(format!(
                "manifest measurement {} execution_transform must be {EXECUTION_TRANSFORM}",
                measurement.id
            )));
        }
        validate_date("as_of", &measurement.as_of)?;
        if measurement.expected_row_count == 0 {
            return Err(AppError::new(format!(
                "manifest measurement {} has zero expected_row_count",
                measurement.id
            )));
        }
        if measurement.expected_sanity.is_empty() {
            return Err(AppError::new(format!(
                "manifest measurement {} must declare expected sanity fields",
                measurement.id
            )));
        }
        if measurement.result_fields.is_empty() {
            return Err(AppError::new(format!(
                "manifest measurement {} must declare result fields",
                measurement.id
            )));
        }
        if !matches!(
            measurement.result_row_validation.as_str(),
            "artifact_digest_only" | "exact_manifest_rows"
        ) {
            return Err(AppError::new(format!(
                "manifest measurement {} has unsupported result_row_validation {}",
                measurement.id, measurement.result_row_validation
            )));
        }
        if measurement.result_row_validation == "artifact_digest_only"
            && measurement.limitations.is_empty()
        {
            return Err(AppError::new(format!(
                "manifest measurement {} must label artifact-digest-only limitations",
                measurement.id
            )));
        }
        if measurement.result_row_validation == "exact_manifest_rows"
            && measurement.expected_result_rows.is_empty()
        {
            return Err(AppError::new(format!(
                "manifest measurement {} must declare expected result rows",
                measurement.id
            )));
        }
        assert_unique_strings(
            &measurement.denominator_fields,
            &format!("{}.denominator_fields", measurement.id),
        )?;
        assert_unique_strings(
            &measurement.result_fields,
            &format!("{}.result_fields", measurement.id),
        )?;
        if measurement.denominator_fields.is_empty() {
            return Err(AppError::new(format!(
                "manifest measurement {} must declare denominator fields",
                measurement.id
            )));
        }
        let denominator_field_set = measurement
            .denominator_fields
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let expected_denominator_set = measurement
            .expected_denominators
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        if expected_denominator_set != denominator_field_set {
            return Err(AppError::new(format!(
                "manifest measurement {} expected_denominators keys must exactly match denominator_fields",
                measurement.id
            )));
        }
        for field in &measurement.denominator_fields {
            match measurement.expected_denominators.get(field) {
                Some(value) if *value > 0 => {}
                Some(_) => {
                    return Err(AppError::new(format!(
                        "manifest measurement {} denominator {} must be nonzero",
                        measurement.id, field
                    )));
                }
                None => {
                    return Err(AppError::new(format!(
                        "manifest measurement {} missing expected denominator {}",
                        measurement.id, field
                    )));
                }
            }
        }
        let allowed = measurement
            .result_fields
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if measurement.result_row_validation == "exact_manifest_rows" {
            for row in &measurement.expected_result_rows {
                let row_keys = row.keys().cloned().collect::<BTreeSet<_>>();
                if row_keys != allowed {
                    return Err(AppError::new(format!(
                        "manifest measurement {} expected row keys must exactly match result_fields",
                        measurement.id
                    )));
                }
            }
        } else if !measurement.expected_result_rows.is_empty() {
            return Err(AppError::new(format!(
                "manifest measurement {} cannot carry expected_result_rows when result_row_validation is artifact_digest_only",
                measurement.id
            )));
        }
        let actual = sha256_file(&repo_root.join(&measurement.sql_path))?;
        if actual != measurement.source_sql_sha256 {
            return Err(AppError::new(format!(
                "SQL drift for {}: {} expected {}, actual {}",
                measurement.id, measurement.sql_path, measurement.source_sql_sha256, actual
            )));
        }
    }
    if seen != declared_set {
        let missing = declared
            .iter()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(AppError::new(format!(
            "manifest is missing required measurement ids: {missing}"
        )));
    }
    Ok(manifest)
}

fn validate_required_measurement_ids(ids: &[String]) -> Result<(), AppError> {
    assert_unique_strings(ids, "required_measurement_ids")?;
    let core = REQUIRED_CORE_MEASUREMENT_IDS
        .iter()
        .copied()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if ids.len() < core.len() {
        return Err(AppError::new(
            "manifest required_measurement_ids is missing mandatory B/C/D/F core measurements",
        ));
    }
    if ids[..core.len()] != core[..] {
        return Err(AppError::new(
            "manifest required_measurement_ids must begin with the mandatory B/C/D/F core prefix in order",
        ));
    }
    let extensions = &ids[core.len()..];
    for id in extensions {
        validate_extension_measurement_id(id)?;
    }
    if extensions.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(AppError::new(
            "manifest extension measurement ids must be lexicographically sorted after the B/C/D/F core prefix",
        ));
    }
    Ok(())
}

fn validate_extension_measurement_id(id: &str) -> Result<(), AppError> {
    let Some((_, version)) = id.rsplit_once("_v") else {
        return Err(AppError::new(format!(
            "extension measurement id {id} must carry a _vN suffix"
        )));
    };
    if version.is_empty() || !version.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::new(format!(
            "extension measurement id {id} must carry a numeric _vN suffix"
        )));
    }
    Ok(())
}

fn validate_gate(measurement_id: &str, gate: &str) -> Result<(), AppError> {
    if matches!(
        gate,
        "G3" | "G4" | "G6" | "G7" | "appendix_b" | "appendix_c" | "appendix_d" | "appendix_f"
    ) {
        return Ok(());
    }
    Err(AppError::new(format!(
        "manifest measurement {measurement_id} gate must name G3, G4, G6, G7, or a retained appendix id"
    )))
}

fn validate_measurement_claim_boundary(measurement: &ManifestMeasurement) -> Result<(), AppError> {
    if !(measurement.id.starts_with("e5_")
        || measurement.section.to_ascii_lowercase().starts_with("e5"))
    {
        return Ok(());
    }
    if measurement.result_row_validation != "exact_manifest_rows" {
        return Err(AppError::new(format!(
            "E5 measurement {} must use exact_manifest_rows so bounded preflight rows are not inferred",
            measurement.id
        )));
    }
    let boundary_text = format!(
        "{} {}",
        measurement.description,
        measurement.limitations.join(" ")
    )
    .to_ascii_lowercase();
    let boundary_phrases = if boundary_text.contains("candidate-reach result") {
        [
            "candidate-reach result",
            "not e5 accuracy",
            "not precision",
            "not four independent votes",
        ]
    } else {
        [
            "bounded source availability",
            "not e5 accuracy",
            "not parcel reach",
            "not four independent votes",
        ]
    };
    for phrase in boundary_phrases {
        if !boundary_text.contains(phrase) {
            return Err(AppError::new(format!(
                "E5 measurement {} must state the {phrase} boundary",
                measurement.id
            )));
        }
    }
    if boundary_text.contains("e5 complete") || boundary_text.contains("live_attested") {
        return Err(AppError::new(format!(
            "E5 measurement {} cannot claim E5 completion or live attestation",
            measurement.id
        )));
    }
    for field in &measurement.result_fields {
        let field = field.to_ascii_lowercase();
        if field.contains("precision")
            || field.contains("accuracy")
            || field.contains("e5_complete")
            || field.contains("live_attested")
        {
            return Err(AppError::new(format!(
                "E5 measurement {} result field {field} would overclaim this source-availability preflight",
                measurement.id
            )));
        }
    }
    Ok(())
}

fn proof_attestation(receipt: &Receipt, query_history: QueryHistoryAttestation) -> String {
    let proof_class = receipt.proof_class.to_ascii_lowercase();
    match (proof_class.as_str(), receipt.query_id.as_ref()) {
        ("observed", None) => "observed".to_string(),
        ("observed", Some(_)) => "invalid_input".to_string(),
        ("cmdrvl_data_live", Some(_)) if query_history == QueryHistoryAttestation::Bound => {
            "LiveComplete".to_string()
        }
        ("cmdrvl_data_live", Some(_)) if query_history == QueryHistoryAttestation::Unattested => {
            "query_history_unattested".to_string()
        }
        ("cmdrvl_data_live", Some(_)) => "invalid_input".to_string(),
        (_, None) => "invalid_input".to_string(),
        _ => "receipt_consistent".to_string(),
    }
}

fn unevaluated_query_history_attestation(receipt: &Receipt) -> QueryHistoryAttestation {
    if receipt.proof_class.eq_ignore_ascii_case("cmdrvl_data_live") && receipt.query_id.is_some() {
        QueryHistoryAttestation::Unattested
    } else {
        QueryHistoryAttestation::NotRequired
    }
}

fn exact_row_diff_detail(
    measurement: &ManifestMeasurement,
    actual_rows: &[BTreeMap<String, Value>],
) -> String {
    let mut detail = format!(
        "grain {} expected {} rows, actual {}",
        measurement.declared_grain,
        measurement.expected_result_rows.len(),
        actual_rows.len()
    );
    let Some(expected_ids) = row_grain_values(
        &measurement.expected_result_rows,
        &measurement.declared_grain,
    ) else {
        detail.push_str("; declared grain is absent from one or more expected rows");
        return detail;
    };
    let Some(actual_ids) = row_grain_values(actual_rows, &measurement.declared_grain) else {
        detail.push_str("; declared grain is absent from one or more actual rows");
        return detail;
    };
    let missing = expected_ids
        .difference(&actual_ids)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = actual_ids
        .difference(&expected_ids)
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        detail.push_str("; missing ids ");
        detail.push_str(&missing.join(", "));
    }
    if !unexpected.is_empty() {
        detail.push_str("; unexpected ids ");
        detail.push_str(&unexpected.join(", "));
    }
    if missing.is_empty() && unexpected.is_empty() {
        detail.push_str("; row literals differ at matching grain values");
    }
    detail
}

fn row_grain_values(rows: &[BTreeMap<String, Value>], field: &str) -> Option<BTreeSet<String>> {
    rows.iter()
        .map(|row| row.get(field).map(display_grain_value))
        .collect()
}

fn display_grain_value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| compact_json(value))
}

fn load_receipts(path: &Path) -> Result<Vec<Receipt>, AppError> {
    let bytes = fs::read(path).map_err(|error| {
        AppError::new(format!(
            "failed to read receipts {}: {error}",
            path.display()
        ))
    })?;
    let bundle: ReceiptBundle = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::new(format!("failed to decode receipt bundle: {error}")))?;
    if bundle.version != RECEIPTS_VERSION {
        return Err(AppError::new(format!(
            "unsupported receipts version {}; expected {RECEIPTS_VERSION}",
            bundle.version
        )));
    }
    Ok(bundle.receipts)
}

fn load_query_history(path: Option<&Path>) -> QueryHistoryLookup {
    let Some(path) = path else {
        return QueryHistoryLookup::unavailable(
            "--query-history was not supplied for cmdrvl_data_live attestation",
        );
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return QueryHistoryLookup::unavailable(format!(
                "failed to read query history {}: {error}",
                path.display()
            ));
        }
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) => {
            return QueryHistoryLookup::unavailable(format!(
                "failed to parse query history {}: {error}",
                path.display()
            ));
        }
    };
    match QueryHistoryLookup::from_value(&value) {
        Ok(history) => history,
        Err(detail) => QueryHistoryLookup::unavailable(format!(
            "failed to decode query history {}: {detail}",
            path.display()
        )),
    }
}

impl QueryHistoryLookup {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            unavailable: Some(reason.into()),
            entries: BTreeMap::new(),
            duplicate_query_ids: BTreeSet::new(),
        }
    }

    fn from_value(value: &Value) -> Result<Self, String> {
        let rows = query_history_rows(value)?;
        let mut entries = BTreeMap::new();
        let mut duplicate_query_ids = BTreeSet::new();
        for (index, row) in rows.iter().enumerate() {
            let object = row
                .as_object()
                .ok_or_else(|| format!("query history row {index} must be an object"))?;
            let query_id =
                required_history_string(object, index, &["query_id", "QUERY_ID"], "query_id")?;
            if !valid_query_id(query_id) {
                return Err(format!(
                    "query history row {index} query_id is empty or noncanonical"
                ));
            }
            let statement_text = required_history_string(
                object,
                index,
                &[
                    "statement_text",
                    "query_text",
                    "QUERY_TEXT",
                    "sql_text",
                    "SQL_TEXT",
                    "query",
                ],
                "statement text",
            )?;
            if statement_text.is_empty() {
                return Err(format!(
                    "query history row {index} statement text must be nonempty"
                ));
            }
            let entry = QueryHistoryEntry {
                statement_text_sha256: sha256_hex(statement_text.as_bytes()),
            };
            if entries.insert(query_id.to_string(), entry).is_some() {
                duplicate_query_ids.insert(query_id.to_string());
            }
        }
        Ok(Self {
            unavailable: None,
            entries,
            duplicate_query_ids,
        })
    }
}

fn query_history_rows(value: &Value) -> Result<&[Value], String> {
    if let Some(rows) = value.as_array() {
        return Ok(rows);
    }
    let object = value
        .as_object()
        .ok_or_else(|| "query history must be an array or object".to_string())?;
    for key in ["queries", "rows", "query_history"] {
        if let Some(rows) = object.get(key).and_then(Value::as_array) {
            return Ok(rows);
        }
    }
    Err("query history object must contain queries, rows, or query_history array".to_string())
}

fn required_history_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    index: usize,
    aliases: &[&str],
    field_name: &str,
) -> Result<&'a str, String> {
    aliases
        .iter()
        .find_map(|alias| object.get(*alias).and_then(Value::as_str))
        .ok_or_else(|| format!("query history row {index} requires string {field_name}"))
}

fn plan_for(manifest: &Manifest) -> MeasurementPlan {
    MeasurementPlan {
        version: PLAN_VERSION.to_string(),
        scope: manifest.scope.clone(),
        offline_only: manifest.offline_only,
        execution: "operator_fed_cmdrvl_data_receipts_only_no_snowflake_execution".to_string(),
        claim_boundary: CLAIM_BOUNDARY.to_string(),
        measurements: manifest
            .measurements
            .iter()
            .enumerate()
            .map(|(index, measurement)| MeasurementPlanRow {
                order: index + 1,
                id: measurement.id.clone(),
                section: measurement.section.clone(),
                gate: measurement.gate.clone(),
                geography: measurement.geography.clone(),
                tier: measurement.tier.clone(),
                declared_grain: measurement.declared_grain.clone(),
                sql_path: measurement.sql_path.clone(),
                source_sql_sha256: measurement.source_sql_sha256.clone(),
                execution_transform: measurement.execution_transform.clone(),
                as_of: measurement.as_of.clone(),
                release_pins: measurement.release_pins.clone(),
                denominator_fields: measurement.denominator_fields.clone(),
                sanity_fields: measurement.expected_sanity.keys().cloned().collect(),
                result_row_validation: measurement.result_row_validation.clone(),
                limitations: measurement.limitations.clone(),
                result_fields: measurement.result_fields.clone(),
            })
            .collect(),
    }
}

fn report_for(
    manifest: &Manifest,
    receipts: &[Receipt],
    receipt_base: &Path,
    query_history: &QueryHistoryLookup,
) -> MeasurementReport {
    let mut by_id: BTreeMap<String, Vec<&Receipt>> = BTreeMap::new();
    let mut by_query_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for receipt in receipts {
        by_id
            .entry(receipt.measurement_id.clone())
            .or_default()
            .push(receipt);
        if let Some(query_id) = &receipt.query_id {
            by_query_id
                .entry(query_id.clone())
                .or_default()
                .push(receipt.measurement_id.clone());
        }
    }
    let duplicate_query_ids = by_query_id
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(query_id, _)| query_id)
        .collect::<BTreeSet<_>>();
    let manifest_ids = manifest
        .measurements
        .iter()
        .map(|measurement| measurement.id.clone())
        .collect::<BTreeSet<_>>();

    let mut rows = Vec::new();
    for (index, measurement) in manifest.measurements.iter().enumerate() {
        let receipts = by_id.remove(&measurement.id).unwrap_or_default();
        rows.push(validate_receipt_row(
            index + 1,
            measurement,
            &receipts,
            &duplicate_query_ids,
            receipt_base,
            query_history,
        ));
    }
    for (id, receipts) in by_id {
        if manifest_ids.contains(&id) {
            continue;
        }
        for receipt in receipts {
            rows.push(MeasurementStatusRow {
                order: None,
                measurement_id: receipt.measurement_id.clone(),
                status: MeasurementStatus::Malformed,
                execution_channel: Some(receipt.execution_channel.clone()),
                execution_transform: Some(receipt.execution_transform.clone()),
                executed_query_text_path: Some(receipt.executed_query_text_path.clone()),
                query_id: receipt.query_id.clone(),
                executed_at: Some(receipt.executed_at.clone()),
                as_of: Some(receipt.as_of.clone()),
                release_pins: Some(receipt.release_pins.clone()),
                declared_proof_class: Some(receipt.proof_class.clone()),
                proof_attestation: Some(proof_attestation(
                    receipt,
                    unevaluated_query_history_attestation(receipt),
                )),
                source_sql_sha256: Some(receipt.source_sql_sha256.clone()),
                executed_query_text_sha256: Some(receipt.executed_query_text_sha256.clone()),
                result_artifact_sha256: receipt.result_artifact_sha256.clone(),
                result_set_sha256: receipt.result_set_sha256.clone(),
                result_validation: None,
                row_count: Some(receipt.row_count),
                details: vec!["receipt measurement_id is not declared in the manifest".to_string()],
            });
        }
    }

    let summary = ReportSummary {
        total: rows.len(),
        receipt_consistent: rows
            .iter()
            .filter(|row| row.status == MeasurementStatus::ReceiptConsistent)
            .count(),
        query_history_unattested: rows
            .iter()
            .filter(|row| row.status == MeasurementStatus::QueryHistoryUnattested)
            .count(),
        snapshot_moved: rows
            .iter()
            .filter(|row| row.status == MeasurementStatus::SnapshotMoved)
            .count(),
        result_mismatch: rows
            .iter()
            .filter(|row| row.status == MeasurementStatus::ResultMismatch)
            .count(),
        malformed: rows
            .iter()
            .filter(|row| row.status == MeasurementStatus::Malformed)
            .count(),
        missing: rows
            .iter()
            .filter(|row| row.status == MeasurementStatus::Missing)
            .count(),
    };
    MeasurementReport {
        version: REPORT_VERSION.to_string(),
        scope: manifest.scope.clone(),
        offline_only: true,
        execution: "offline_validation_only_no_snowflake_execution".to_string(),
        claim_boundary: CLAIM_BOUNDARY.to_string(),
        summary,
        measurements: rows,
    }
}

fn validate_receipt_row(
    order: usize,
    measurement: &ManifestMeasurement,
    receipts: &[&Receipt],
    duplicate_query_ids: &BTreeSet<String>,
    receipt_base: &Path,
    query_history: &QueryHistoryLookup,
) -> MeasurementStatusRow {
    if receipts.is_empty() {
        return MeasurementStatusRow {
            order: Some(order),
            measurement_id: measurement.id.clone(),
            status: MeasurementStatus::Missing,
            execution_channel: None,
            execution_transform: None,
            executed_query_text_path: None,
            query_id: None,
            executed_at: None,
            as_of: None,
            release_pins: None,
            declared_proof_class: None,
            proof_attestation: None,
            source_sql_sha256: None,
            executed_query_text_sha256: None,
            result_artifact_sha256: None,
            result_set_sha256: None,
            result_validation: Some(measurement.result_row_validation.clone()),
            row_count: None,
            details: vec!["operator receipt is missing".to_string()],
        };
    }
    if receipts.len() > 1 {
        return MeasurementStatusRow {
            order: Some(order),
            measurement_id: measurement.id.clone(),
            status: MeasurementStatus::Malformed,
            execution_channel: None,
            execution_transform: None,
            executed_query_text_path: None,
            query_id: None,
            executed_at: None,
            as_of: None,
            release_pins: None,
            declared_proof_class: None,
            proof_attestation: None,
            source_sql_sha256: None,
            executed_query_text_sha256: None,
            result_artifact_sha256: None,
            result_set_sha256: None,
            result_validation: Some(measurement.result_row_validation.clone()),
            row_count: None,
            details: vec!["duplicate receipt measurement_id".to_string()],
        };
    }

    let receipt = receipts[0];
    let mut malformed = Vec::new();
    let mut mismatches = Vec::new();
    let mut unattested = Vec::new();
    let mut artifact_rows = Vec::new();
    let mut artifact_loaded = false;
    let mut query_id_is_bindable = false;

    if receipt.source_sql_sha256 != measurement.source_sql_sha256 {
        malformed.push(format!(
            "source SQL digest drift: receipt {}, manifest {}",
            receipt.source_sql_sha256, measurement.source_sql_sha256
        ));
    }
    if let Err(error) = validate_sha256(
        "executed_query_text_sha256",
        &receipt.executed_query_text_sha256,
    ) {
        malformed.push(error.message);
    }
    if receipt.execution_channel != EXECUTION_CHANNEL {
        malformed.push(format!(
            "execution_channel must be {EXECUTION_CHANNEL}, actual {}",
            receipt.execution_channel
        ));
    }
    if receipt.execution_transform != measurement.execution_transform
        || receipt.execution_transform != EXECUTION_TRANSFORM
    {
        malformed.push(format!(
            "execution_transform must be {EXECUTION_TRANSFORM}, actual {}",
            receipt.execution_transform
        ));
    }
    if !valid_datetime(&receipt.executed_at) {
        malformed.push("executed_at must be an RFC3339-like UTC/offset timestamp".to_string());
    }
    if !valid_date(&receipt.as_of) {
        malformed.push("as_of must be YYYY-MM-DD".to_string());
    }
    let proof_class = receipt.proof_class.to_ascii_lowercase();
    if !matches!(
        proof_class.as_str(),
        "contract_fixture" | "live" | "fresh_live" | "cmdrvl_data_live" | "observed"
    ) {
        malformed.push(
            "proof_class must be contract_fixture, live, fresh_live, cmdrvl_data_live, or observed"
                .to_string(),
        );
    }
    match (&receipt.query_id, proof_class.as_str()) {
        (None, "observed") => {}
        (None, proof_class) => malformed.push(format!(
            "query_id missing for proof_class {proof_class}; only observed may omit it"
        )),
        (Some(_), "observed") => {
            malformed.push("proof_class observed requires query_id null".to_string());
        }
        (Some(query_id), _) => {
            if duplicate_query_ids.contains(query_id) {
                malformed.push("duplicate query_id across receipts".to_string());
            }
            if !valid_query_id(query_id) {
                malformed.push("query_id is empty or noncanonical".to_string());
            } else if !duplicate_query_ids.contains(query_id) {
                query_id_is_bindable = true;
            }
        }
    }
    let query_history_attestation = if proof_class == "cmdrvl_data_live" && query_id_is_bindable {
        match bind_query_history(measurement, receipt, query_history) {
            QueryHistoryBinding::Bound => QueryHistoryAttestation::Bound,
            QueryHistoryBinding::Unattested(detail) => {
                unattested.push(detail);
                QueryHistoryAttestation::Unattested
            }
            QueryHistoryBinding::Refused(detail) => {
                malformed.push(detail);
                QueryHistoryAttestation::Refused
            }
        }
    } else {
        QueryHistoryAttestation::NotRequired
    };
    match load_result_artifact(receipt_base, measurement, receipt) {
        Ok(artifact) => {
            artifact_loaded = true;
            artifact_rows = artifact.rows;
        }
        Err(detail) => malformed.push(detail),
    }

    if !receipt.gate_values.is_empty() {
        malformed
            .push("manifest v0 declares no gate fields; gate_values must be empty".to_string());
    }

    if artifact_loaded {
        validate_artifact_row_fields(measurement, &artifact_rows, &mut malformed);
        match u64::try_from(artifact_rows.len()) {
            Ok(artifact_row_count) if receipt.row_count == artifact_row_count => {}
            Ok(artifact_row_count) => {
                malformed.push(format!(
                    "receipt row_count {} does not match artifact row count {}",
                    receipt.row_count, artifact_row_count
                ));
            }
            Err(_) => malformed.push("artifact row count overflow".to_string()),
        }
        if receipt.row_count == 0 {
            malformed.push("row_count is zero on a green receipt".to_string());
        }

        match derive_denominators(measurement, &artifact_rows) {
            Ok(derived) => {
                validate_receipt_denominators(measurement, receipt, &derived, &mut malformed);
                validate_expected_denominators(measurement, &derived, &mut mismatches);
            }
            Err(detail) => malformed.push(detail),
        }
        match derive_sanity(measurement, &artifact_rows) {
            Ok(derived) => {
                validate_receipt_sanity(measurement, receipt, &derived, &mut malformed);
                validate_expected_sanity(measurement, &derived, &mut mismatches);
            }
            Err(detail) => malformed.push(detail),
        }

        if receipt.row_count != measurement.expected_row_count {
            mismatches.push(format!(
                "row_count mismatch: expected {}, actual {}",
                measurement.expected_row_count, receipt.row_count
            ));
        }
        if measurement.result_row_validation == "exact_manifest_rows"
            && canonical_rows(&artifact_rows) != canonical_rows(&measurement.expected_result_rows)
        {
            mismatches.push(exact_row_diff_detail(measurement, &artifact_rows));
        }
    }

    let (status, details) = if !malformed.is_empty() {
        (MeasurementStatus::Malformed, malformed)
    } else if receipt.release_pins != measurement.release_pins || receipt.as_of != measurement.as_of
    {
        (
            MeasurementStatus::SnapshotMoved,
            vec![
                "release pins or as_of differ from the manifest; record this as a new measurement"
                    .to_string(),
            ],
        )
    } else if !mismatches.is_empty() {
        (MeasurementStatus::ResultMismatch, mismatches)
    } else if !unattested.is_empty() {
        (MeasurementStatus::QueryHistoryUnattested, unattested)
    } else {
        let mut details = if query_history_attestation == QueryHistoryAttestation::Bound {
            vec![QUERY_HISTORY_BOUND.to_string()]
        } else {
            vec![LIVENESS_NOT_ATTESTED.to_string()]
        };
        if measurement.result_row_validation != "exact_manifest_rows" {
            details.extend(measurement.limitations.clone());
        }
        (MeasurementStatus::ReceiptConsistent, details)
    };

    MeasurementStatusRow {
        order: Some(order),
        measurement_id: measurement.id.clone(),
        status,
        execution_channel: Some(receipt.execution_channel.clone()),
        execution_transform: Some(receipt.execution_transform.clone()),
        executed_query_text_path: Some(receipt.executed_query_text_path.clone()),
        query_id: receipt.query_id.clone(),
        executed_at: Some(receipt.executed_at.clone()),
        as_of: Some(receipt.as_of.clone()),
        release_pins: Some(receipt.release_pins.clone()),
        declared_proof_class: Some(receipt.proof_class.clone()),
        proof_attestation: Some(proof_attestation(receipt, query_history_attestation)),
        source_sql_sha256: Some(receipt.source_sql_sha256.clone()),
        executed_query_text_sha256: Some(receipt.executed_query_text_sha256.clone()),
        result_artifact_sha256: receipt.result_artifact_sha256.clone(),
        result_set_sha256: receipt.result_set_sha256.clone(),
        result_validation: Some(measurement.result_row_validation.clone()),
        row_count: Some(receipt.row_count),
        details,
    }
}

fn bind_query_history(
    measurement: &ManifestMeasurement,
    receipt: &Receipt,
    query_history: &QueryHistoryLookup,
) -> QueryHistoryBinding {
    let query_id = receipt
        .query_id
        .as_deref()
        .expect("caller checks query_id exists before binding");
    if let Some(reason) = &query_history.unavailable {
        return QueryHistoryBinding::Unattested(format!(
            "query history unavailable for measurement_id {} query_id {}: {reason}",
            measurement.id, query_id
        ));
    }
    if query_history.duplicate_query_ids.contains(query_id) {
        return QueryHistoryBinding::Unattested(format!(
            "query history has duplicate query_id {} for measurement_id {}; cannot bind exactly one statement",
            query_id, measurement.id
        ));
    }
    let Some(entry) = query_history.entries.get(query_id) else {
        return QueryHistoryBinding::Unattested(format!(
            "query history has no row for measurement_id {} query_id {}; history may be expired or incomplete",
            measurement.id, query_id
        ));
    };
    if entry.statement_text_sha256 == receipt.executed_query_text_sha256 {
        QueryHistoryBinding::Bound
    } else {
        QueryHistoryBinding::Refused(format!(
            "query_id correspondence mismatch for measurement_id {} query_id {}: expected executed_query_text_sha256 {}, actual query_history statement_text_sha256 {}",
            measurement.id,
            query_id,
            receipt.executed_query_text_sha256,
            entry.statement_text_sha256
        ))
    }
}

fn load_result_artifact(
    receipt_base: &Path,
    measurement: &ManifestMeasurement,
    receipt: &Receipt,
) -> Result<ResultArtifact, String> {
    let path = receipt
        .result_artifact_path
        .as_deref()
        .ok_or_else(|| "receipt requires result_artifact_path".to_string())?;
    let expected_artifact_sha256 = receipt
        .result_artifact_sha256
        .as_deref()
        .ok_or_else(|| "receipt requires result_artifact_sha256".to_string())?;
    let expected_result_set_sha256 = receipt
        .result_set_sha256
        .as_deref()
        .ok_or_else(|| "receipt requires result_set_sha256".to_string())?;
    validate_sha256("result_artifact_sha256", expected_artifact_sha256)
        .map_err(|error| error.message)?;
    validate_sha256("result_set_sha256", expected_result_set_sha256)
        .map_err(|error| error.message)?;
    validate_sha256(
        "executed_query_text_sha256",
        &receipt.executed_query_text_sha256,
    )
    .map_err(|error| error.message)?;
    validate_relative_executed_query_text_path(&receipt.executed_query_text_path)?;
    validate_relative_artifact_path(path)?;

    let executed_query_text_path = receipt_base.join(&receipt.executed_query_text_path);
    let executed_query_text_bytes = fs::read(&executed_query_text_path).map_err(|error| {
        format!(
            "failed to read executed query text artifact {}: {error}",
            executed_query_text_path.display()
        )
    })?;
    let actual_executed_query_text_sha256 = sha256_hex(&executed_query_text_bytes);
    if actual_executed_query_text_sha256 != receipt.executed_query_text_sha256 {
        return Err(format!(
            "executed query text SHA-256 mismatch: receipt {}, actual {}",
            receipt.executed_query_text_sha256, actual_executed_query_text_sha256
        ));
    }

    let artifact_path = receipt_base.join(path);
    let artifact_bytes = fs::read(&artifact_path).map_err(|error| {
        format!(
            "failed to read result artifact {}: {error}",
            artifact_path.display()
        )
    })?;
    let actual_artifact_sha256 = sha256_hex(&artifact_bytes);
    if actual_artifact_sha256 != expected_artifact_sha256 {
        return Err(format!(
            "result artifact SHA-256 mismatch: receipt {expected_artifact_sha256}, actual {actual_artifact_sha256}"
        ));
    }
    let artifact: ResultArtifact = serde_json::from_slice(&artifact_bytes).map_err(|error| {
        format!(
            "failed to decode result artifact {}: {error}",
            artifact_path.display()
        )
    })?;

    if artifact.version != RESULT_ARTIFACT_VERSION {
        return Err(format!(
            "unsupported result artifact version {}; expected {RESULT_ARTIFACT_VERSION}",
            artifact.version
        ));
    }
    if artifact.measurement_id != receipt.measurement_id
        || artifact.measurement_id != measurement.id
        || artifact.execution_channel != receipt.execution_channel
        || artifact.execution_channel != EXECUTION_CHANNEL
        || artifact.execution_transform != receipt.execution_transform
        || artifact.execution_transform != measurement.execution_transform
        || artifact.execution_transform != EXECUTION_TRANSFORM
        || artifact.executed_query_text_path != receipt.executed_query_text_path
        || artifact.query_id != receipt.query_id
        || artifact.source_sql_sha256 != receipt.source_sql_sha256
        || artifact.source_sql_sha256 != measurement.source_sql_sha256
        || artifact.executed_query_text_sha256 != receipt.executed_query_text_sha256
    {
        return Err("result artifact binding fields do not match receipt and manifest".to_string());
    }
    if artifact.rows.is_empty() {
        return Err("result artifact rows must be nonempty".to_string());
    }
    let actual_result_set_sha256 = result_set_sha256(
        &artifact.measurement_id,
        &artifact.source_sql_sha256,
        &artifact.executed_query_text_sha256,
        &artifact.rows,
    )
    .map_err(|error| error.message)?;
    if actual_result_set_sha256 != expected_result_set_sha256 {
        return Err(format!(
            "result-set SHA-256 mismatch: receipt {expected_result_set_sha256}, actual {actual_result_set_sha256}"
        ));
    }
    Ok(artifact)
}

fn validate_artifact_row_fields(
    measurement: &ManifestMeasurement,
    rows: &[BTreeMap<String, Value>],
    malformed: &mut Vec<String>,
) {
    let result_fields = measurement
        .result_fields
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for (index, row) in rows.iter().enumerate() {
        let row_fields = row.keys().cloned().collect::<BTreeSet<_>>();
        if row_fields != result_fields {
            malformed.push(format!(
                "artifact row {index} keys must exactly match manifest result_fields"
            ));
        }
    }
}

fn validate_receipt_denominators(
    measurement: &ManifestMeasurement,
    receipt: &Receipt,
    derived: &BTreeMap<String, u64>,
    malformed: &mut Vec<String>,
) {
    let declared = measurement
        .denominator_fields
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let receipt_keys = receipt
        .denominators
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if receipt_keys != declared {
        malformed
            .push("receipt denominator keys must exactly match denominator_fields".to_string());
        return;
    }
    for (field, actual) in derived {
        match receipt.denominators.get(field) {
            Some(value) if value == actual && *value > 0 => {}
            Some(value) if *value == 0 => {
                malformed.push(format!("denominator {field} is zero on a green receipt"));
            }
            Some(value) => malformed.push(format!(
                "receipt denominator {field}={value} does not match artifact-derived {actual}"
            )),
            None => malformed.push(format!("missing denominator field {field}")),
        }
    }
}

fn validate_expected_denominators(
    measurement: &ManifestMeasurement,
    derived: &BTreeMap<String, u64>,
    mismatches: &mut Vec<String>,
) {
    for (field, expected) in &measurement.expected_denominators {
        match derived.get(field) {
            Some(actual) if actual == expected => {}
            Some(actual) => mismatches.push(format!(
                "denominator {field} mismatch: expected {expected}, artifact-derived {actual}"
            )),
            None => mismatches.push(format!("denominator {field} was not derived from artifact")),
        }
    }
}

fn validate_receipt_sanity(
    measurement: &ManifestMeasurement,
    receipt: &Receipt,
    derived: &BTreeMap<String, Value>,
    malformed: &mut Vec<String>,
) {
    let declared = measurement
        .expected_sanity
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let receipt_keys = receipt.sanity.keys().cloned().collect::<BTreeSet<_>>();
    if receipt_keys != declared {
        malformed.push("receipt sanity keys must exactly match manifest sanity fields".to_string());
        return;
    }
    for (field, actual) in derived {
        match receipt.sanity.get(field) {
            Some(value) if value == actual => {}
            Some(value) => malformed.push(format!(
                "receipt sanity {field}={} does not match artifact-derived {}",
                compact_json(value),
                compact_json(actual)
            )),
            None => malformed.push(format!("missing sanity field {field}")),
        }
    }
}

fn validate_expected_sanity(
    measurement: &ManifestMeasurement,
    derived: &BTreeMap<String, Value>,
    mismatches: &mut Vec<String>,
) {
    for (field, expected) in &measurement.expected_sanity {
        match derived.get(field) {
            Some(actual) if actual == expected => {}
            Some(actual) => mismatches.push(format!(
                "sanity field {field} mismatch: expected {}, artifact-derived {}",
                compact_json(expected),
                compact_json(actual)
            )),
            None => mismatches.push(format!(
                "sanity field {field} was not derived from artifact"
            )),
        }
    }
}

fn derive_denominators(
    measurement: &ManifestMeasurement,
    rows: &[BTreeMap<String, Value>],
) -> Result<BTreeMap<String, u64>, String> {
    let mut denominators = BTreeMap::new();
    match measurement.id.as_str() {
        "appendix_b_centroid_percolation" => {
            denominators.insert(
                "observation_count".to_string(),
                u64::try_from(rows.len()).map_err(|_| "row count overflow".to_string())?,
            );
        }
        "appendix_c_r8_density" => {
            let row = single_row(measurement, rows)?;
            for field in &measurement.denominator_fields {
                denominators.insert(field.clone(), required_u64(row, field)?);
            }
        }
        "appendix_d_same_cell_predicates" | "appendix_d_candidate_reach" => {
            denominators.insert(
                "total_footprints".to_string(),
                sum_u64(rows, "footprint_count")?,
            );
        }
        "appendix_d_stratified_halo_centers" => {
            denominators.insert(
                "selected_center_count".to_string(),
                u64::try_from(rows.len()).map_err(|_| "row count overflow".to_string())?,
            );
        }
        "appendix_d_stratified_halo" => {
            denominators.insert(
                "r8_target_footprints".to_string(),
                sum_u64_where(rows, "target_footprints", "resolution", &Value::from(8))?,
            );
            denominators.insert(
                "r9_target_footprints".to_string(),
                sum_u64_where(rows, "target_footprints", "resolution", &Value::from(9))?,
            );
        }
        "appendix_f_overture_three_source" => {
            denominators.insert(
                "total_center_observations".to_string(),
                sum_u64(rows, "target_observations")?,
            );
            denominators.insert(
                "overture_osm_lineage_observations".to_string(),
                sum_u64_where(
                    rows,
                    "osm_lineage_observations",
                    "source_name",
                    &Value::from("overture_building"),
                )?,
            );
        }
        "e5_franklin_county_thin_tier_readiness_v0" => {
            for field in &measurement.denominator_fields {
                let value = if let Some((evidence_class, source_field)) = field.split_once('.') {
                    let row = single_evidence_class_row(measurement, rows, evidence_class)?;
                    required_u64(row, source_field)?
                } else {
                    shared_u64(rows, field)?
                };
                denominators.insert(field.clone(), value);
            }
        }
        "e5_franklin_county_parcel_candidate_reach_v0"
        | "e5_franklin_deed_truth_export_v0"
        | "e5_microsoft_globalml_franklin_h3_coverage_v0" => {
            let row = single_row(measurement, rows)?;
            for field in &measurement.denominator_fields {
                denominators.insert(field.clone(), required_u64(row, field)?);
            }
        }
        other => {
            return Err(format!(
                "no denominator derivation is declared for measurement {other}"
            ));
        }
    }
    Ok(denominators)
}

fn derive_sanity(
    measurement: &ManifestMeasurement,
    rows: &[BTreeMap<String, Value>],
) -> Result<BTreeMap<String, Value>, String> {
    let mut sanity = BTreeMap::new();
    for (field, expected) in &measurement.expected_sanity {
        let derived = if field == "artifact_row_count_matches_expected" {
            let row_count =
                u64::try_from(rows.len()).map_err(|_| "row count overflow".to_string())?;
            Value::Bool(row_count == measurement.expected_row_count)
        } else if expected.as_str().is_some() {
            if all_rows_equal(rows, field, expected)? {
                expected.clone()
            } else {
                Value::from("FAIL")
            }
        } else if expected.as_u64().is_some() || expected.as_i64().is_some() {
            Value::from(sum_u64(rows, field)?)
        } else if expected.as_bool().is_some() {
            Value::Bool(all_rows_equal(rows, field, expected)?)
        } else {
            return Err(format!("unsupported sanity expectation type for {field}"));
        };
        sanity.insert(field.clone(), derived);
    }
    Ok(sanity)
}

fn single_row<'a>(
    measurement: &ManifestMeasurement,
    rows: &'a [BTreeMap<String, Value>],
) -> Result<&'a BTreeMap<String, Value>, String> {
    if rows.len() != 1 {
        return Err(format!(
            "measurement {} expected a single-row artifact for this derivation, got {}",
            measurement.id,
            rows.len()
        ));
    }
    Ok(&rows[0])
}

fn single_evidence_class_row<'a>(
    measurement: &ManifestMeasurement,
    rows: &'a [BTreeMap<String, Value>],
    evidence_class: &str,
) -> Result<&'a BTreeMap<String, Value>, String> {
    let mut matches = rows
        .iter()
        .filter(|row| row.get("evidence_class").and_then(Value::as_str) == Some(evidence_class));
    let Some(row) = matches.next() else {
        return Err(format!(
            "measurement {} is missing evidence_class {evidence_class}",
            measurement.id
        ));
    };
    if matches.next().is_some() {
        return Err(format!(
            "measurement {} has duplicate evidence_class {evidence_class}",
            measurement.id
        ));
    }
    Ok(row)
}

fn shared_u64(rows: &[BTreeMap<String, Value>], field: &str) -> Result<u64, String> {
    let mut values = rows.iter().map(|row| required_u64(row, field));
    let Some(first) = values.next() else {
        return Err(format!(
            "cannot derive shared denominator {field} from empty rows"
        ));
    };
    let first = first?;
    for value in values {
        let value = value?;
        if value != first {
            return Err(format!(
                "shared denominator {field} differs across artifact rows"
            ));
        }
    }
    Ok(first)
}

fn sum_u64(rows: &[BTreeMap<String, Value>], field: &str) -> Result<u64, String> {
    rows.iter().try_fold(0_u64, |sum, row| {
        let value = required_u64(row, field)?;
        sum.checked_add(value)
            .ok_or_else(|| format!("sum overflow for field {field}"))
    })
}

fn sum_u64_where(
    rows: &[BTreeMap<String, Value>],
    field: &str,
    selector: &str,
    expected_selector: &Value,
) -> Result<u64, String> {
    rows.iter().try_fold(0_u64, |sum, row| {
        if row.get(selector) != Some(expected_selector) {
            return Ok(sum);
        }
        let value = required_u64(row, field)?;
        sum.checked_add(value)
            .ok_or_else(|| format!("sum overflow for field {field}"))
    })
}

fn all_rows_equal(
    rows: &[BTreeMap<String, Value>],
    field: &str,
    expected: &Value,
) -> Result<bool, String> {
    for row in rows {
        let Some(value) = row.get(field) else {
            return Err(format!("artifact row is missing sanity field {field}"));
        };
        if value != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

fn required_u64(row: &BTreeMap<String, Value>, field: &str) -> Result<u64, String> {
    row.get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("artifact row field {field} must be a nonnegative integer"))
}

fn result_set_sha256(
    measurement_id: &str,
    source_sql_sha256: &str,
    executed_query_text_sha256: &str,
    rows: &[BTreeMap<String, Value>],
) -> Result<String, AppError> {
    let view = CanonicalResultSet {
        version: RESULT_SET_VERSION,
        measurement_id,
        source_sql_sha256,
        executed_query_text_sha256,
        rows: canonical_rows(rows),
    };
    serde_json::to_vec(&view)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| {
            AppError::new(format!("failed to serialize canonical result set: {error}"))
        })
}

fn canonical_rows(rows: &[BTreeMap<String, Value>]) -> Vec<BTreeMap<String, Value>> {
    let mut rows = rows.to_vec();
    rows.sort_by_key(compact_json_map);
    rows
}

fn compact_json_map(row: &BTreeMap<String, Value>) -> String {
    serde_json::to_string(row).unwrap_or_else(|_| "<unserializable>".to_string())
}

fn absolute_path(path: &Path) -> Result<PathBuf, AppError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| AppError::new(format!("failed to resolve current directory: {error}")))
    }
}

fn resolve_repo_relative(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

fn validate_relative_sql_path(path: &str) -> Result<(), AppError> {
    validate_relative_path(path, "sql")
}

fn validate_relative_artifact_path(path: &str) -> Result<(), String> {
    validate_relative_path(path, "json").map_err(|error| error.message)
}

fn validate_relative_executed_query_text_path(path: &str) -> Result<(), String> {
    validate_relative_path_with_extensions(path, &["sql", "txt"]).map_err(|error| error.message)
}

fn validate_relative_path(path: &str, extension: &str) -> Result<(), AppError> {
    validate_relative_path_with_extensions(path, &[extension])
}

fn validate_relative_path_with_extensions(path: &str, extensions: &[&str]) -> Result<(), AppError> {
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(AppError::new(format!(
            "absolute path is not allowed: {}",
            path.display()
        )));
    }
    let mut saw_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => saw_component = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::new(format!(
                    "path traversal is not allowed: {}",
                    path.display()
                )));
            }
        }
    }
    let has_allowed_extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|extension| extensions.contains(&extension));
    if !saw_component || !has_allowed_extension {
        return Err(AppError::new(format!(
            "path must name a relative file with one of these extensions {:?}: {}",
            extensions,
            path.display(),
        )));
    }
    Ok(())
}

fn assert_unique_strings(values: &[String], field: &str) -> Result<(), AppError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.is_empty() {
            return Err(AppError::new(format!("{field} contains an empty value")));
        }
        if !seen.insert(value) {
            return Err(AppError::new(format!(
                "{field} contains duplicate value {value}"
            )));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let bytes = fs::read(path).map_err(|error| {
        AppError::new(format!("failed to read SQL {}: {error}", path.display()))
    })?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_sha256(field: &str, value: &str) -> Result<(), AppError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::new(format!(
            "{field} must be lowercase SHA-256 hex"
        )));
    }
    Ok(())
}

fn valid_query_id(value: &str) -> bool {
    if value.len() != 36 || value.trim() != value {
        return false;
    }
    for (index, byte) in value.bytes().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte) {
            return false;
        }
    }
    true
}

fn validate_date(field: &str, value: &str) -> Result<(), AppError> {
    if valid_date(value) {
        Ok(())
    } else {
        Err(AppError::new(format!("{field} must be YYYY-MM-DD")))
    }
}

fn valid_date(value: &str) -> bool {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

fn valid_datetime(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok()
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".to_string())
}

fn print_json(value: &impl Serialize) -> Result<(), AppError> {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, value)
        .map_err(|error| AppError::new(format!("failed to serialize output: {error}")))?;
    writeln!(&mut lock).map_err(|error| AppError::new(format!("failed to write output: {error}")))
}

fn write_canonical(bytes: &[u8]) -> Result<(), AppError> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(bytes)
        .map_err(|error| AppError::new(format!("failed to write output: {error}")))?;
    stdout
        .write_all(b"\n")
        .map_err(|error| AppError::new(format!("failed to write output: {error}")))?;
    stdout
        .flush()
        .map_err(|error| AppError::new(format!("failed to flush output: {error}")))
}
