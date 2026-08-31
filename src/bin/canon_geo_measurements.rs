#![forbid(unsafe_code)]

use chrono::{DateTime, NaiveDate};
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

const MANIFEST_VERSION: &str = "canon_geo_measurement_manifest.v0";
const PLAN_VERSION: &str = "canon_geo_measurement_plan.v0";
const REPORT_VERSION: &str = "canon_geo_measurement_report.v0";
const RECEIPTS_VERSION: &str = "canon_geo_measurement_receipts.v0";
const RESULT_ARTIFACT_VERSION: &str = "canon_geo_measurement_result_artifact.v0";
const RESULT_SET_VERSION: &str = "canon_geo_measurement_result_set.v0";
const EXECUTION_CHANNEL: &str = "cmdrvl_data_mcp";
const EXECUTION_TRANSFORM: &str = "cmdrvl_data_sqlglot_normalized_plus_tool_row_limit";
const LIVENESS_NOT_ATTESTED: &str = "receipt is internally consistent, but this offline runner does not attest liveness, authenticity, or query-history provenance";
const CLAIM_BOUNDARY: &str = "Offline receipt consistency validation only. A receipt_consistent row means the receipt is bound to result artifact bytes and executed query text bytes, and matches the manifest's declared offline checks. source_sql_sha256 is the local file byte digest; executed_query_text_sha256 is recomputed from the supplied normalized query text artifact after the declared cmdrvl-data/Snowflake transform. This proves byte integrity, not authenticity or liveness. result_set_sha256 is over an unordered canonical result set sorted deterministically by compact JSON row encoding. Integration-test positive JSON is a contract fixture, not live proof of cmdrvl-data execution.";
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
    #[arg(long, value_enum)]
    emit: Option<EmitMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum EmitMode {
    Plan,
    Report,
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
    query_id: String,
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
    query_id: String,
    source_sql_sha256: String,
    executed_query_text_sha256: String,
    rows: Vec<BTreeMap<String, Value>>,
}

#[derive(Serialize)]
struct CanonicalResultSet<'a> {
    version: &'static str,
    measurement_id: &'a str,
    source_sql_sha256: &'a str,
    executed_query_text_sha256: &'a str,
    rows: Vec<BTreeMap<String, Value>>,
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
            let receipt_base = receipts_path.parent().unwrap_or_else(|| Path::new("."));
            let report = report_for(&manifest, &receipts, receipt_base);
            let ok = report.summary.snapshot_moved == 0
                && report.summary.result_mismatch == 0
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
    for phrase in [
        "bounded source availability",
        "not e5 accuracy",
        "not parcel reach",
        "not four independent votes",
    ] {
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

fn report_for(manifest: &Manifest, receipts: &[Receipt], receipt_base: &Path) -> MeasurementReport {
    let mut by_id: BTreeMap<String, Vec<&Receipt>> = BTreeMap::new();
    let mut by_query_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for receipt in receipts {
        by_id
            .entry(receipt.measurement_id.clone())
            .or_default()
            .push(receipt);
        by_query_id
            .entry(receipt.query_id.clone())
            .or_default()
            .push(receipt.measurement_id.clone());
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
                query_id: Some(receipt.query_id.clone()),
                executed_at: Some(receipt.executed_at.clone()),
                as_of: Some(receipt.as_of.clone()),
                release_pins: Some(receipt.release_pins.clone()),
                declared_proof_class: Some(receipt.proof_class.clone()),
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
    let mut artifact_rows = Vec::new();
    let mut artifact_loaded = false;

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
    if duplicate_query_ids.contains(&receipt.query_id) {
        malformed.push("duplicate query_id across receipts".to_string());
    }
    if !valid_query_id(&receipt.query_id) {
        malformed.push("query_id is empty or noncanonical".to_string());
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
        "contract_fixture" | "live" | "fresh_live" | "cmdrvl_data_live"
    ) {
        malformed.push(
            "proof_class must be contract_fixture, live, fresh_live, or cmdrvl_data_live"
                .to_string(),
        );
    }
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
            mismatches.push("artifact rows differ from manifest expected_result_rows".to_string());
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
    } else {
        let mut details = vec![LIVENESS_NOT_ATTESTED.to_string()];
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
        query_id: Some(receipt.query_id.clone()),
        executed_at: Some(receipt.executed_at.clone()),
        as_of: Some(receipt.as_of.clone()),
        release_pins: Some(receipt.release_pins.clone()),
        declared_proof_class: Some(receipt.proof_class.clone()),
        source_sql_sha256: Some(receipt.source_sql_sha256.clone()),
        executed_query_text_sha256: Some(receipt.executed_query_text_sha256.clone()),
        result_artifact_sha256: receipt.result_artifact_sha256.clone(),
        result_set_sha256: receipt.result_set_sha256.clone(),
        result_validation: Some(measurement.result_row_validation.clone()),
        row_count: Some(receipt.row_count),
        details,
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
    use std::io::Write as _;
    writeln!(&mut lock).map_err(|error| AppError::new(format!("failed to write output: {error}")))
}
