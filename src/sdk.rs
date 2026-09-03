//! Embeddable API for immutable Canon artifacts and exact lookup.
//!
//! SDK requests carry an explicit `api_version` field. Omitting it is a
//! compile-time error for callers constructing requests directly:
//!
//! ```compile_fail
//! use canon::sdk::ExactMappingRequest;
//! use std::path::PathBuf;
//!
//! let _request = ExactMappingRequest {
//!     input_path: PathBuf::from("input.csv"),
//!     registry_path: PathBuf::from("registry"),
//!     column: "cusip".to_string(),
//!     limits: Default::default(),
//!     explicit: false,
//!     plain_json_values: false,
//! };
//! ```
//!
//! ```compile_fail
//! use canon::sdk::EntityScorePairRequest;
//! use serde_json::json;
//! use std::path::PathBuf;
//!
//! let _request = EntityScorePairRequest {
//!     left: json!({"source_row_id": "left"}),
//!     right: json!({"source_row_id": "right"}),
//!     profile: "cmbs_tenant_label".to_string(),
//!     strategy: PathBuf::from("strategy.yaml"),
//!     registry: None,
//! };
//! ```

use crate::{
    CanonOutput, InputFormat, Mapping, Outcome, Refusal, RefusalCode, RegistryMeta, ResolveResult,
    SpecialReason, Summary, UnresolvedEntry, distribution, entity, input, lookup, output, project,
    refusal, registry, witness,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

pub const DEFAULT_BATCH_RECORD_LIMIT: usize = 100_000;
pub const DEFAULT_ARTIFACT_BYTE_LIMIT: u64 = 16 * 1024 * 1024;
pub const MAX_PAGE_LIMIT: usize = 10_000;

pub type SdkResult<T> = Result<T, Box<SdkRefusal>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SdkApiVersion {
    V1,
}

impl SdkApiVersion {
    pub const fn v1() -> Self {
        Self::V1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

impl Default for InputLimits {
    fn default() -> Self {
        Self {
            max_rows: Some(DEFAULT_BATCH_RECORD_LIMIT),
            max_bytes: Some(DEFAULT_ARTIFACT_BYTE_LIMIT),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLimits {
    pub max_bytes: u64,
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_ARTIFACT_BYTE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRequest {
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl PageRequest {
    pub fn first(limit: usize) -> Self {
        Self {
            limit,
            cursor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageInfo {
    pub total: usize,
    pub returned: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SdkRefusal {
    pub code: RefusalCode,
    pub message: String,
    pub detail: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_command: Option<String>,
    pub envelope: CanonOutput,
}

impl SdkRefusal {
    pub fn as_envelope_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.envelope)
    }
}

impl fmt::Display for SdkRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for SdkRefusal {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactMappingRequest {
    pub api_version: SdkApiVersion,
    pub input_path: PathBuf,
    pub registry_path: PathBuf,
    pub column: String,
    #[serde(default)]
    pub limits: InputLimits,
    #[serde(default)]
    pub explicit: bool,
    #[serde(default)]
    pub plain_json_values: bool,
}

impl ExactMappingRequest {
    pub fn v1(input_path: PathBuf, registry_path: PathBuf, column: impl Into<String>) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            input_path,
            registry_path,
            column: column.into(),
            limits: InputLimits::default(),
            explicit: false,
            plain_json_values: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExactMappingResponse {
    pub api_version: SdkApiVersion,
    pub exit_code: u8,
    pub outcome: Outcome,
    pub registry: RegistryMeta,
    pub summary: Summary,
    pub artifact_json: Vec<u8>,
    pub result: SdkLookupResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowPreservingCsvMappingRequest {
    pub api_version: SdkApiVersion,
    pub input_path: PathBuf,
    pub registry_path: PathBuf,
    pub column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canon_column: Option<String>,
    #[serde(default)]
    pub limits: InputLimits,
    #[serde(default)]
    pub explicit_mapping_artifact: bool,
}

impl RowPreservingCsvMappingRequest {
    pub fn v1(input_path: PathBuf, registry_path: PathBuf, column: impl Into<String>) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            input_path,
            registry_path,
            column: column.into(),
            canon_column: None,
            limits: InputLimits::default(),
            explicit_mapping_artifact: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowPreservingCsvMappingResponse {
    pub api_version: SdkApiVersion,
    pub exit_code: u8,
    pub outcome: Outcome,
    pub registry: RegistryMeta,
    pub summary: Summary,
    pub csv_bytes: Vec<u8>,
    pub mapping_artifact_json: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBatchLookupRequest {
    pub api_version: SdkApiVersion,
    pub registry_path: PathBuf,
    pub values: Vec<String>,
    #[serde(default)]
    pub max_values: Option<usize>,
    #[serde(default)]
    pub explicit: bool,
    #[serde(default)]
    pub plain_json_values: bool,
}

impl ExactBatchLookupRequest {
    pub fn v1(registry_path: PathBuf, values: Vec<String>) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            registry_path,
            values,
            max_values: Some(DEFAULT_BATCH_RECORD_LIMIT),
            explicit: false,
            plain_json_values: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExactBatchLookupResponse {
    pub api_version: SdkApiVersion,
    pub exit_code: u8,
    pub outcome: Outcome,
    pub registry: RegistryMeta,
    pub summary: Summary,
    pub artifact_json: Vec<u8>,
    pub result: SdkLookupResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SdkLookupResult {
    pub mappings: Vec<Mapping>,
    pub unresolved: Vec<UnresolvedEntry>,
    pub summary: Summary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageOpenRequest {
    pub api_version: SdkApiVersion,
    pub archive_path: PathBuf,
    #[serde(default)]
    pub limits: ReadLimits,
}

impl PackageOpenRequest {
    pub fn v1(archive_path: PathBuf) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            archive_path,
            limits: ReadLimits::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageOpenResponse {
    pub api_version: SdkApiVersion,
    pub inspection: distribution::package::LocalPackageInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageVerifyRequest {
    pub api_version: SdkApiVersion,
    pub archive_path: PathBuf,
    #[serde(default)]
    pub limits: ReadLimits,
}

impl PackageVerifyRequest {
    pub fn v1(archive_path: PathBuf) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            archive_path,
            limits: ReadLimits::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageVerifyResponse {
    pub api_version: SdkApiVersion,
    pub verification: distribution::package::LocalPackageVerification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Artifact,
    Evidence,
    Explanation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactReadRequest {
    pub api_version: SdkApiVersion,
    pub path: PathBuf,
    pub kind: ArtifactKind,
    #[serde(default)]
    pub limits: ReadLimits,
}

impl ArtifactReadRequest {
    pub fn v1(path: PathBuf, kind: ArtifactKind) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            path,
            kind,
            limits: ReadLimits::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactReadResponse {
    pub api_version: SdkApiVersion,
    pub kind: ArtifactKind,
    pub path: PathBuf,
    pub byte_count: u64,
    pub content_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_version: Option<String>,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryMetadataRequest {
    pub api_version: SdkApiVersion,
    pub registry_path: PathBuf,
    #[serde(default)]
    pub limits: ReadLimits,
}

impl RegistryMetadataRequest {
    pub fn v1(registry_path: PathBuf) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            registry_path,
            limits: ReadLimits::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryMetadataResponse {
    pub api_version: SdkApiVersion,
    pub id: String,
    pub version: String,
    pub source: String,
    pub entry_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_iri_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_id_scheme: Option<registry::DefaultIdScheme>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunEventsRequest {
    pub api_version: SdkApiVersion,
    pub run_report_path: PathBuf,
    #[serde(default)]
    pub limits: ReadLimits,
    pub page: PageRequest,
}

impl ProjectRunEventsRequest {
    pub fn v1(run_report_path: PathBuf, page: PageRequest) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            run_report_path,
            limits: ReadLimits::default(),
            page,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunEvent {
    pub project_id: String,
    pub plan_graph_hash: String,
    pub node_id: String,
    pub outcome: project::ProjectRunNodeOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunEventsResponse {
    pub api_version: SdkApiVersion,
    pub run_schema_version: String,
    pub project_id: String,
    pub plan_graph_hash: String,
    pub page: PageInfo,
    pub events: Vec<ProjectRunEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityScorePairRequest {
    pub api_version: SdkApiVersion,
    pub left: Value,
    pub right: Value,
    pub profile: String,
    pub strategy: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<PathBuf>,
}

impl EntityScorePairRequest {
    pub fn v1(left: Value, right: Value, profile: impl Into<String>, strategy: PathBuf) -> Self {
        Self {
            api_version: SdkApiVersion::v1(),
            left,
            right,
            profile: profile.into(),
            strategy,
            registry: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityScorePairVerdict {
    CannotLink,
    WouldMerge,
    WouldAttach,
    WouldEscrow,
    BelowFloor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityScorePairThresholds {
    pub backbone_score_min: u32,
    pub attach_score_min: u32,
    pub abstain_margin: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityScorePairResponse {
    pub api_version: SdkApiVersion,
    pub profile_id: String,
    pub profile_version: String,
    pub strategy_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_snapshot_hash: Option<String>,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: u32,
    pub verdict: EntityScorePairVerdict,
    pub thresholds: EntityScorePairThresholds,
    pub evidence_record: entity::edge::EdgeEvidenceRecord,
    pub evidence_waterfall: entity::review_export::NativeEvidenceWaterfall,
    pub writes_performed: bool,
}

pub fn exact_mapping_artifact(request: ExactMappingRequest) -> SdkResult<ExactMappingResponse> {
    let resolved = resolve_file(
        &request.input_path,
        &request.registry_path,
        &request.column,
        request.limits,
    )?;
    let artifact_json = mapping_artifact_bytes(
        &resolved.registry_meta,
        &resolved.result,
        request.explicit,
        request.plain_json_values,
    )?;
    let result = sorted_result(resolved.result);
    let outcome = crate::determine_outcome(&result.summary);
    Ok(ExactMappingResponse {
        api_version: request.api_version,
        exit_code: exit_code(outcome.clone()),
        outcome,
        registry: resolved.registry_meta,
        summary: result.summary.clone(),
        artifact_json,
        result: SdkLookupResult::from(result),
    })
}

pub fn row_preserving_csv_mapping(
    request: RowPreservingCsvMappingRequest,
) -> SdkResult<RowPreservingCsvMappingResponse> {
    let resolved = resolve_file(
        &request.input_path,
        &request.registry_path,
        &request.column,
        request.limits,
    )?;
    if matches!(resolved.input_values.format, InputFormat::Jsonl) {
        return Err(sdk_refusal_from_canon(refusal::create_refusal(
            RefusalCode::EEmitFormat,
            "--emit csv cannot be used with JSONL input".to_string(),
            serde_json::json!({"input_format": "jsonl", "emit_mode": "csv"}),
            Some("Use --emit json with JSONL input".to_string()),
        )));
    }

    let resolve_map = crate::build_resolve_map(&resolved.result);
    let default_canonical_column = format!("{}__canon", request.column);
    let canonical_column = request
        .canon_column
        .as_deref()
        .unwrap_or(default_canonical_column.as_str());
    let mut csv_bytes = Vec::new();
    output::csv::emit_csv(
        &request.input_path,
        &resolve_map,
        &request.column,
        canonical_column,
        resolved.input_values.delimiter.unwrap_or(b','),
        &mut csv_bytes,
    )
    .map_err(crate::create_csv_output_refusal)
    .map_err(sdk_refusal_from_canon)?;

    let mapping_artifact_json = mapping_artifact_bytes(
        &resolved.registry_meta,
        &resolved.result,
        request.explicit_mapping_artifact,
        false,
    )?;
    let outcome = crate::determine_outcome(&resolved.result.summary);
    Ok(RowPreservingCsvMappingResponse {
        api_version: request.api_version,
        exit_code: exit_code(outcome.clone()),
        outcome,
        registry: resolved.registry_meta,
        summary: resolved.result.summary,
        csv_bytes,
        mapping_artifact_json,
    })
}

pub fn exact_batch_lookup(request: ExactBatchLookupRequest) -> SdkResult<ExactBatchLookupResponse> {
    let max_values = request.max_values.unwrap_or(DEFAULT_BATCH_RECORD_LIMIT);
    if request.values.len() > max_values {
        return Err(too_large_refusal(
            "max_rows",
            max_values.to_string(),
            request.values.len().to_string(),
        ));
    }
    if request.values.is_empty() {
        return Err(sdk_refusal_from_canon(refusal::create_refusal(
            RefusalCode::EEmptyInput,
            "Input has no processable rows".to_string(),
            serde_json::json!({}),
            None,
        )));
    }

    let registry = registry::load_registry(&request.registry_path)
        .map_err(crate::create_registry_refusal)
        .map_err(sdk_refusal_from_canon)?;
    let input_values = input_values_from_batch(&request.values);
    let result = lookup::resolve_values(&registry, &input_values)
        .map_err(crate::create_lookup_refusal)
        .map_err(sdk_refusal_from_canon)?;
    let result = sorted_result(result);
    let artifact_json = mapping_artifact_bytes(
        &registry.meta,
        &result,
        request.explicit,
        request.plain_json_values,
    )?;
    let outcome = crate::determine_outcome(&result.summary);

    Ok(ExactBatchLookupResponse {
        api_version: request.api_version,
        exit_code: exit_code(outcome.clone()),
        outcome,
        registry: registry.meta,
        summary: result.summary.clone(),
        artifact_json,
        result: SdkLookupResult::from(result),
    })
}

pub fn open_package(request: PackageOpenRequest) -> SdkResult<PackageOpenResponse> {
    let archive_bytes = read_bounded_file(
        &request.archive_path,
        request.limits.max_bytes,
        "package archive",
    )?;
    let inspection =
        distribution::package::inspect_local_package(&archive_bytes).map_err(package_refusal)?;
    Ok(PackageOpenResponse {
        api_version: request.api_version,
        inspection,
    })
}

pub fn verify_package(request: PackageVerifyRequest) -> SdkResult<PackageVerifyResponse> {
    let archive_bytes = read_bounded_file(
        &request.archive_path,
        request.limits.max_bytes,
        "package archive",
    )?;
    let verification =
        distribution::package::verify_local_package(&archive_bytes).map_err(package_refusal)?;
    Ok(PackageVerifyResponse {
        api_version: request.api_version,
        verification,
    })
}

pub fn read_artifact(request: ArtifactReadRequest) -> SdkResult<ArtifactReadResponse> {
    let bytes = read_bounded_file(&request.path, request.limits.max_bytes, "artifact")?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        sdk_refusal_from_canon(refusal::create_refusal(
            RefusalCode::EParse,
            format!(
                "Failed to parse artifact '{}': {error}",
                request.path.display()
            ),
            serde_json::json!({
                "path": request.path.display().to_string(),
                "artifact": format!("{:?}", request.kind),
                "error": error.to_string(),
            }),
            None,
        ))
    })?;
    let declared_version = value
        .get("schema_version")
        .or_else(|| value.get("version"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(ArtifactReadResponse {
        api_version: request.api_version,
        kind: request.kind,
        path: request.path,
        byte_count: bytes.len() as u64,
        content_digest: witness::hash_bytes(&bytes),
        declared_version,
        value,
    })
}

pub fn read_registry_metadata(
    request: RegistryMetadataRequest,
) -> SdkResult<RegistryMetadataResponse> {
    let registry_json_path = request.registry_path.join("registry.json");
    let bytes = read_bounded_file(
        &registry_json_path,
        request.limits.max_bytes,
        "registry metadata",
    )?;
    let metadata: RegistryMetadataJson = serde_json::from_slice(&bytes).map_err(|error| {
        sdk_refusal_from_canon(refusal::create_refusal(
            RefusalCode::EBadRegistry,
            format!(
                "Failed to parse registry metadata '{}': {error}",
                registry_json_path.display()
            ),
            serde_json::json!({
                "path": registry_json_path.display().to_string(),
                "error": error.to_string(),
            }),
            None,
        ))
    })?;

    Ok(RegistryMetadataResponse {
        api_version: request.api_version,
        id: metadata.id,
        version: metadata.version,
        source: request.registry_path.to_string_lossy().into_owned(),
        entry_count: metadata.entry_count,
        canonical_iri_namespace: metadata.canonical_iri_namespace,
        default_id_scheme: metadata.default_id_scheme,
    })
}

pub fn read_project_run_events(
    request: ProjectRunEventsRequest,
) -> SdkResult<ProjectRunEventsResponse> {
    let bytes = read_bounded_file(
        &request.run_report_path,
        request.limits.max_bytes,
        "project run report",
    )?;
    let report: project::ProjectRunReport = serde_json::from_slice(&bytes).map_err(|error| {
        sdk_refusal_from_canon(refusal::create_refusal(
            RefusalCode::EParse,
            format!(
                "Failed to parse project run report '{}': {error}",
                request.run_report_path.display()
            ),
            serde_json::json!({
                "path": request.run_report_path.display().to_string(),
                "artifact": "project run report",
                "error": error.to_string(),
            }),
            None,
        ))
    })?;

    let mut events = report
        .node_reports
        .iter()
        .map(|node| ProjectRunEvent {
            project_id: report.project_id.clone(),
            plan_graph_hash: report.plan_graph_hash.clone(),
            node_id: node.node_id.clone(),
            outcome: node.outcome,
            receipt_hash: node.receipt_hash.clone(),
            reason: node.reason.clone(),
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        left.node_id
            .cmp(&right.node_id)
            .then(left.receipt_hash.cmp(&right.receipt_hash))
            .then(left.reason.cmp(&right.reason))
    });

    let (events, page) = paginate(events, request.page)?;
    Ok(ProjectRunEventsResponse {
        api_version: request.api_version,
        run_schema_version: report.schema_version,
        project_id: report.project_id,
        plan_graph_hash: report.plan_graph_hash,
        page,
        events,
    })
}

pub fn score_entity_pair(request: EntityScorePairRequest) -> SdkResult<EntityScorePairResponse> {
    let evaluation = entity::score_pair::score_pair(entity::score_pair::ScorePairRequest {
        left: &request.left,
        right: &request.right,
        profile: &request.profile,
        strategy: &request.strategy,
        registry: request.registry.as_deref(),
    })
    .map_err(sdk_refusal_from_refusal)?;

    Ok(EntityScorePairResponse {
        api_version: request.api_version,
        profile_id: evaluation.profile_id,
        profile_version: evaluation.profile_version,
        strategy_hash: evaluation.strategy_hash,
        registry_snapshot_hash: evaluation.registry_snapshot_hash,
        left_surface_id: evaluation.left_surface_id,
        right_surface_id: evaluation.right_surface_id,
        score_units: evaluation.score_units,
        verdict: EntityScorePairVerdict::from(evaluation.verdict),
        thresholds: EntityScorePairThresholds::from(evaluation.thresholds),
        evidence_record: evaluation.evidence_record,
        evidence_waterfall: evaluation.evidence_waterfall,
        writes_performed: false,
    })
}

struct ResolvedFile {
    registry_meta: RegistryMeta,
    input_values: crate::InputValues,
    result: ResolveResult,
}

fn resolve_file(
    input_path: &Path,
    registry_path: &Path,
    column: &str,
    limits: InputLimits,
) -> SdkResult<ResolvedFile> {
    let registry = registry::load_registry(registry_path)
        .map_err(crate::create_registry_refusal)
        .map_err(sdk_refusal_from_canon)?;
    let input_values = input::parse_input(input_path, column, limits.max_bytes, limits.max_rows)
        .map_err(crate::create_input_refusal)
        .map_err(sdk_refusal_from_canon)?;
    let result = lookup::resolve_values(&registry, &input_values)
        .map_err(crate::create_lookup_refusal)
        .map_err(sdk_refusal_from_canon)?;
    Ok(ResolvedFile {
        registry_meta: registry.meta,
        input_values,
        result,
    })
}

fn input_values_from_batch(values: &[String]) -> crate::InputValues {
    let mut unique = HashMap::new();
    let mut empty_count = 0usize;
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            empty_count += 1;
        } else {
            unique.insert(trimmed.to_string(), ());
        }
    }
    let mut special = HashMap::new();
    if empty_count > 0 {
        special.insert(SpecialReason::EmptyValue, empty_count);
    }
    crate::InputValues {
        values: unique,
        special,
        format: InputFormat::Csv,
        delimiter: Some(b','),
        source_hash: None,
        source_bytes: None,
    }
}

fn mapping_artifact_bytes(
    registry_meta: &RegistryMeta,
    result: &ResolveResult,
    explicit: bool,
    plain_json_values: bool,
) -> SdkResult<Vec<u8>> {
    output::json::emit_json_explicit_with_plain_values(
        registry_meta,
        result,
        explicit,
        plain_json_values,
    )
    .map(String::into_bytes)
    .map_err(crate::create_output_refusal)
    .map_err(sdk_refusal_from_canon)
}

fn sorted_result(mut result: ResolveResult) -> ResolveResult {
    result
        .mappings
        .sort_by(|left, right| left.input.cmp(&right.input));
    result
        .unresolved
        .sort_by(|left, right| match (&left.input, &right.input) {
            (Some(left_input), Some(right_input)) => left_input.cmp(right_input),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => left.reason.cmp(&right.reason),
        });
    result
}

impl From<ResolveResult> for SdkLookupResult {
    fn from(result: ResolveResult) -> Self {
        Self {
            mappings: result.mappings,
            unresolved: result.unresolved,
            summary: result.summary,
        }
    }
}

fn exit_code(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Resolved => 0,
        Outcome::Partial | Outcome::Unresolved => 1,
        Outcome::Refusal => 2,
    }
}

fn read_bounded_file(path: &Path, max_bytes: u64, label: &str) -> SdkResult<Vec<u8>> {
    if let Ok(metadata) = fs::metadata(path) {
        let actual = metadata.len();
        if actual > max_bytes {
            return Err(too_large_refusal(
                "max_bytes",
                max_bytes.to_string(),
                actual.to_string(),
            ));
        }
    }

    let bytes = fs::read(path).map_err(|error| {
        sdk_refusal_from_canon(refusal::create_refusal(
            RefusalCode::EIo,
            format!("Unable to read {label} '{}': {error}", path.display()),
            serde_json::json!({
                "path": path.display().to_string(),
                "artifact": label,
                "error": error.to_string(),
            }),
            None,
        ))
    })?;
    if bytes.len() as u64 > max_bytes {
        return Err(too_large_refusal(
            "max_bytes",
            max_bytes.to_string(),
            bytes.len().to_string(),
        ));
    }
    Ok(bytes)
}

fn paginate<T>(items: Vec<T>, page: PageRequest) -> SdkResult<(Vec<T>, PageInfo)> {
    if page.limit == 0 || page.limit > MAX_PAGE_LIMIT {
        return Err(too_large_refusal(
            "max_rows",
            MAX_PAGE_LIMIT.to_string(),
            page.limit.to_string(),
        ));
    }
    let start = match page.cursor.as_deref() {
        Some(cursor) => cursor.parse::<usize>().map_err(|error| {
            sdk_refusal_from_canon(refusal::create_refusal(
                RefusalCode::EParse,
                format!("Invalid SDK page cursor '{cursor}': {error}"),
                serde_json::json!({ "cursor": cursor, "error": error.to_string() }),
                None,
            ))
        })?,
        None => 0,
    };
    if start > items.len() {
        return Err(sdk_refusal_from_canon(refusal::create_refusal(
            RefusalCode::EParse,
            format!(
                "SDK page cursor {} is beyond {} available records",
                start,
                items.len()
            ),
            serde_json::json!({ "cursor": start, "total": items.len() }),
            None,
        )));
    }

    let end = start.saturating_add(page.limit).min(items.len());
    let total = items.len();
    let returned = end - start;
    let next_cursor = (end < total).then(|| end.to_string());
    Ok((
        items.into_iter().skip(start).take(returned).collect(),
        PageInfo {
            total,
            returned,
            next_cursor,
        },
    ))
}

fn package_refusal(error: distribution::package::LocalPackageError) -> Box<SdkRefusal> {
    let code = match error.kind {
        distribution::package::LocalPackageErrorKind::NonCanonicalPackageBytes => {
            RefusalCode::EPackageNonCanonical
        }
        _ => RefusalCode::EPackageContract,
    };
    sdk_refusal_from_canon(refusal::create_refusal(
        code,
        error.message,
        serde_json::json!({ "kind": format!("{:?}", error.kind) }),
        None,
    ))
}

fn too_large_refusal(limit_type: &str, limit: String, actual: String) -> Box<SdkRefusal> {
    sdk_refusal_from_canon(Refusal::too_large(limit_type, &limit, &actual).to_canon_output())
}

fn sdk_refusal_from_refusal(refusal: Refusal) -> Box<SdkRefusal> {
    sdk_refusal_from_canon(refusal.to_canon_output())
}

fn sdk_refusal_from_canon(output: CanonOutput) -> Box<SdkRefusal> {
    let refusal = output.refusal.as_ref().map(|refusal| (**refusal).clone());
    let refusal = refusal.unwrap_or_else(|| Refusal {
        code: RefusalCode::EIo,
        message: "SDK operation failed without a refusal envelope".to_string(),
        detail: serde_json::json!({}),
        next_command: Some(RefusalCode::EIo.default_next_command().to_string()),
    });
    Box::new(SdkRefusal {
        code: refusal.code,
        message: refusal.message,
        detail: refusal.detail,
        next_command: refusal.next_command,
        envelope: output,
    })
}

#[derive(Debug, Deserialize)]
struct RegistryMetadataJson {
    id: String,
    version: String,
    entry_count: usize,
    #[serde(default)]
    canonical_iri_namespace: Option<String>,
    #[serde(default)]
    default_id_scheme: Option<registry::DefaultIdScheme>,
}

impl From<entity::score_pair::ScorePairVerdict> for EntityScorePairVerdict {
    fn from(verdict: entity::score_pair::ScorePairVerdict) -> Self {
        match verdict {
            entity::score_pair::ScorePairVerdict::CannotLink => Self::CannotLink,
            entity::score_pair::ScorePairVerdict::WouldMerge => Self::WouldMerge,
            entity::score_pair::ScorePairVerdict::WouldAttach => Self::WouldAttach,
            entity::score_pair::ScorePairVerdict::WouldEscrow => Self::WouldEscrow,
            entity::score_pair::ScorePairVerdict::BelowFloor => Self::BelowFloor,
        }
    }
}

impl From<entity::score_pair::ScorePairThresholds> for EntityScorePairThresholds {
    fn from(thresholds: entity::score_pair::ScorePairThresholds) -> Self {
        Self {
            backbone_score_min: thresholds.backbone_score_min,
            attach_score_min: thresholds.attach_score_min,
            abstain_margin: thresholds.abstain_margin,
        }
    }
}

#[allow(dead_code)]
fn assert_public_types_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ExactMappingRequest>();
    assert_send_sync::<ExactMappingResponse>();
    assert_send_sync::<RowPreservingCsvMappingRequest>();
    assert_send_sync::<RowPreservingCsvMappingResponse>();
    assert_send_sync::<ExactBatchLookupRequest>();
    assert_send_sync::<ExactBatchLookupResponse>();
    assert_send_sync::<PackageOpenRequest>();
    assert_send_sync::<PackageOpenResponse>();
    assert_send_sync::<PackageVerifyRequest>();
    assert_send_sync::<PackageVerifyResponse>();
    assert_send_sync::<ArtifactReadRequest>();
    assert_send_sync::<ArtifactReadResponse>();
    assert_send_sync::<RegistryMetadataRequest>();
    assert_send_sync::<RegistryMetadataResponse>();
    assert_send_sync::<ProjectRunEventsRequest>();
    assert_send_sync::<ProjectRunEventsResponse>();
    assert_send_sync::<EntityScorePairRequest>();
    assert_send_sync::<EntityScorePairResponse>();
    assert_send_sync::<SdkRefusal>();
    assert_send_sync::<Box<SdkRefusal>>();
}
