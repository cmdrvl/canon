//! Apply-stage streaming exact replay helpers.
//!
//! This module owns the bounded row adapter only. Promotion, audit validation,
//! and registry mutation safety are separate ENT-P09 surfaces; apply receives a
//! deterministic replay table and appends canonical fields without rewriting
//! the raw input row bytes.

use crate::entity::{
    CANON_ENTITY_APPLY_VERSION, CANON_ENTITY_APPLY_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1,
    CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactReferenceV1, EntityArtifactStageV1,
    EntityDeterministicSummary, entity_artifact_v1_contract_for_legacy_version,
    entity_artifact_v1_contract_for_version,
    error::EntityRefusalKind,
    schema::{
        entity_v1_artifact_reference, entity_v1_lifecycle_metadata_from_source,
        finalize_entity_v1_self_hash,
    },
    stream::{
        EntityStreamChunkMetadata, EntityStreamFormat, EntityStreamInput,
        EntityStreamRowProvenance, EntityStreamStage, EntityStreamTelemetry,
        deterministic_chunk_metadata, stream_telemetry,
    },
};
use crate::{Refusal, witness};
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_APPLY_ROWS_PER_CHUNK: u64 = 1024;
pub const APPLY_CANONICAL_FIELDS: &[&str] = &[
    "canonical_id",
    "canonical_type",
    "canonical_status",
    "canonical_registry_id",
    "canonical_registry_version",
    "canonical_rule_id",
];
pub const SEC10D_ORG_FIELD_SUFFIXES: &[&str] = &[
    "_org_canon_id",
    "_org_canonical_name",
    "_org_resolution_status",
    "_org_registry_id",
    "_org_registry_version",
    "_org_rule_id",
];

const MAX_APPLY_PROVENANCE_SAMPLES: usize = 16;
const APPLY_TEMP_FILE_STALE_AFTER: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ApplyRegistryReference {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyCanonicalResolution {
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ApplySafetyCheck {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_identity_semantics: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_identity_semantics: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_registry_snapshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_registry_snapshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sidecar_artifact_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_sidecar_artifact_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sidecar_snapshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_sidecar_snapshot_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApplyStreamRequest<'a> {
    pub rows: &'a Path,
    pub output: &'a Path,
    pub lookup_column: &'a str,
    pub registry: ApplyRegistryReference,
    pub resolutions: &'a BTreeMap<String, ApplyCanonicalResolution>,
    pub safety: ApplySafetyCheck,
    pub require_full_resolution: bool,
    pub target_rows_per_chunk: u64,
}

#[derive(Debug, Clone)]
pub struct ApplyRegistryStreamRequest<'a> {
    pub rows: &'a Path,
    pub output: &'a Path,
    pub lookup_column: &'a str,
    pub registry_dir: &'a Path,
    pub safety: ApplySafetyCheck,
    pub require_full_resolution: bool,
    pub target_rows_per_chunk: u64,
}

#[derive(Debug, Clone)]
pub struct ApplyV1ArtifactRequest<'a> {
    pub source_artifact: &'a Value,
    pub rows: &'a Path,
    pub output: &'a Path,
    pub lookup_column: &'a str,
    pub registry_dir: &'a Path,
    pub require_full_resolution: bool,
    pub target_rows_per_chunk: u64,
}

#[derive(Debug, Clone)]
pub struct Sec10dOrgApplyStreamRequest<'a> {
    pub rows: &'a Path,
    pub output: &'a Path,
    pub lookup_column: &'a str,
    pub field_name_column: &'a str,
    pub registry: ApplyRegistryReference,
    pub resolutions: &'a BTreeMap<String, Sec10dOrgApplyResolution>,
    pub safety: ApplySafetyCheck,
    pub require_full_resolution: bool,
    pub target_rows_per_chunk: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sec10dOrgApplyResolution {
    pub canonical_id: String,
    pub canonical_name: String,
    pub resolution_status: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyStreamingDiagnostics {
    pub input: EntityStreamInput,
    pub chunks: Vec<EntityStreamChunkMetadata>,
    pub telemetry: EntityStreamTelemetry,
    pub provenance_samples: Vec<EntityStreamRowProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyRunArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub registry: ApplyRegistryReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_snapshot_hash: Option<String>,
    #[serde(default)]
    pub output_content_hash: String,
    pub summary: BTreeMap<String, u64>,
    pub streaming: ApplyStreamingDiagnostics,
    pub output_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApplyStreamingRunData {
    registry: ApplyRegistryReference,
    registry_snapshot_hash: Option<String>,
    output_content_hash: String,
    summary: BTreeMap<String, u64>,
    streaming: ApplyStreamingDiagnostics,
    output_path: String,
    lookup_column: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyInputFormat {
    Csv(u8),
    Jsonl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApplyInputInspection {
    row_count: u64,
    resolved: u64,
    unresolved: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApplyWriteResult {
    row_count: u64,
    resolved: u64,
    unresolved: u64,
    provenance_samples: Vec<EntityStreamRowProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineEnding {
    body_len: usize,
    ending_len: usize,
}

pub fn run_apply_streaming(request: ApplyStreamRequest<'_>) -> Result<ApplyRunArtifact, Refusal> {
    let data = run_apply_streaming_data(request)?;
    let mut artifact = ApplyRunArtifact {
        version: CANON_ENTITY_APPLY_VERSION.to_string(),
        artifact_content_hash: String::new(),
        registry: data.registry,
        registry_snapshot_hash: data.registry_snapshot_hash,
        output_content_hash: data.output_content_hash,
        summary: data.summary,
        streaming: data.streaming,
        output_path: data.output_path,
    };
    artifact.artifact_content_hash = hash_apply_artifact_without_self(&artifact)?;
    Ok(artifact)
}

fn run_apply_streaming_data(
    request: ApplyStreamRequest<'_>,
) -> Result<ApplyStreamingRunData, Refusal> {
    let output = request.output.to_path_buf();
    run_apply_streaming_data_to(request, &output, &output)
}

fn run_apply_streaming_data_to(
    request: ApplyStreamRequest<'_>,
    write_output: &Path,
    logical_output: &Path,
) -> Result<ApplyStreamingRunData, Refusal> {
    let format = apply_input_format(request.rows)?;
    validate_apply_safety(&request)?;
    let inspection = inspect_apply_input(
        request.rows,
        request.lookup_column,
        request.resolutions,
        format,
    )?;
    if request.require_full_resolution && inspection.unresolved > 0 {
        return Err(apply_unresolved_refusal(&request, &inspection));
    }
    let byte_count = fs::metadata(request.rows)
        .map_err(|error| {
            io_budget_refusal(
                "Failed to inspect apply input rows",
                request.rows,
                error.to_string(),
            )
        })?
        .len();
    let content_hash = witness::hash_file(request.rows).map_err(|error| {
        io_budget_refusal(
            "Failed to hash apply input rows",
            request.rows,
            error.to_string(),
        )
    })?;
    let input = EntityStreamInput::new(
        EntityStreamStage::Apply,
        entity_stream_format(format),
        request.rows.display().to_string(),
        content_hash,
        inspection.row_count,
        byte_count,
    );
    let chunks = deterministic_chunk_metadata(&input, request.target_rows_per_chunk)?;
    let telemetry = stream_telemetry(&input, &chunks);

    let write_result = write_apply_output(&request, format, &chunks, write_output)?;
    debug_assert_eq!(write_result.row_count, inspection.row_count);
    let output_content_hash = hash_apply_output(write_output)?;

    Ok(ApplyStreamingRunData {
        registry: request.registry.clone(),
        registry_snapshot_hash: request.safety.actual_registry_snapshot_hash.clone(),
        output_content_hash,
        summary: BTreeMap::from([
            ("rows".to_string(), write_result.row_count),
            ("resolved".to_string(), write_result.resolved),
            ("unresolved".to_string(), write_result.unresolved),
        ]),
        streaming: ApplyStreamingDiagnostics {
            input,
            chunks,
            telemetry,
            provenance_samples: write_result.provenance_samples,
        },
        output_path: logical_output.display().to_string(),
        lookup_column: request.lookup_column.to_string(),
    })
}

pub fn run_apply_streaming_from_registry(
    request: ApplyRegistryStreamRequest<'_>,
) -> Result<ApplyRunArtifact, Refusal> {
    let registry = crate::registry::load_registry(request.registry_dir).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply could not load the versioned registry",
            json!({
                "stage": "apply",
                "registry": request.registry_dir.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Fix registry.json or mapping files, then rerun canon entity apply".to_string()),
        )
    })?;
    let resolutions = apply_resolutions_from_registry(&registry)?;
    let registry_snapshot_hash = apply_registry_snapshot_hash(request.registry_dir)?;
    let (profile_id, identity_semantics) = apply_registry_profile_metadata(request.registry_dir)?;
    let mut safety = request.safety;
    if safety.actual_registry_snapshot_hash.is_none() {
        safety.actual_registry_snapshot_hash = Some(registry_snapshot_hash);
    }
    if safety.actual_profile_id.is_none() {
        safety.actual_profile_id = profile_id;
    }
    if safety.actual_identity_semantics.is_none() {
        safety.actual_identity_semantics = identity_semantics;
    }

    run_apply_streaming(ApplyStreamRequest {
        rows: request.rows,
        output: request.output,
        lookup_column: request.lookup_column,
        registry: ApplyRegistryReference {
            id: registry.meta.id,
            version: registry.meta.version,
        },
        resolutions: &resolutions,
        safety,
        require_full_resolution: request.require_full_resolution,
        target_rows_per_chunk: request.target_rows_per_chunk,
    })
}

pub fn run_apply_v1_from_registry(request: ApplyV1ArtifactRequest<'_>) -> Result<Value, Refusal> {
    run_apply_v1_from_registry_with_builder(request, build_apply_v1_artifact)
}

fn run_apply_v1_from_registry_with_builder<F>(
    request: ApplyV1ArtifactRequest<'_>,
    build_artifact: F,
) -> Result<Value, Refusal>
where
    F: FnOnce(&Value, EntityArtifactReferenceV1, ApplyStreamingRunData) -> Result<Value, Refusal>,
{
    let source_reference = validate_apply_v1_source_artifact(request.source_artifact)?;
    let registry_snapshot_hash = apply_registry_snapshot_hash(request.registry_dir)?;
    let (actual_registry_id, actual_registry_version) =
        apply_registry_identity_metadata(request.registry_dir)?;
    validate_apply_v1_registry_binding(
        request.source_artifact,
        &actual_registry_id,
        &actual_registry_version,
        &registry_snapshot_hash,
    )?;

    let registry = crate::registry::load_registry(request.registry_dir).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply could not load the versioned registry",
            json!({
                "stage": "apply",
                "registry": request.registry_dir.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Fix registry.json or mapping files, then rerun canon entity apply".to_string()),
        )
    })?;
    let resolutions = apply_resolutions_from_registry(&registry)?;
    let (profile_id, identity_semantics) = apply_registry_profile_metadata(request.registry_dir)?;

    let mut safety = apply_v1_safety_check(request.source_artifact);
    safety.actual_registry_snapshot_hash = Some(registry_snapshot_hash);
    safety.actual_profile_id = profile_id;
    safety.actual_identity_semantics = identity_semantics;
    validate_apply_v1_safety_material(&safety)?;
    validate_apply_v1_input_binding(
        request.source_artifact,
        request.rows,
        request.lookup_column,
        &resolutions,
    )?;

    let staged_output = apply_temp_sibling(request.output);
    let data = match run_apply_streaming_data_to(
        ApplyStreamRequest {
            rows: request.rows,
            output: request.output,
            lookup_column: request.lookup_column,
            registry: ApplyRegistryReference {
                id: registry.meta.id,
                version: registry.meta.version,
            },
            resolutions: &resolutions,
            safety,
            require_full_resolution: request.require_full_resolution,
            target_rows_per_chunk: request.target_rows_per_chunk,
        },
        &staged_output,
        request.output,
    ) {
        Ok(data) => data,
        Err(refusal) => {
            cleanup_owned_staged_apply_output(&staged_output);
            return Err(refusal);
        }
    };

    let artifact = match build_artifact(request.source_artifact, source_reference, data) {
        Ok(artifact) => artifact,
        Err(refusal) => {
            cleanup_owned_staged_apply_output(&staged_output);
            return Err(refusal);
        }
    };
    if let Err(refusal) = publish_staged_apply_output(&staged_output, request.output) {
        cleanup_owned_staged_apply_output(&staged_output);
        return Err(refusal);
    }

    Ok(artifact)
}

fn validate_apply_v1_source_artifact(
    source_artifact: &Value,
) -> Result<EntityArtifactReferenceV1, Refusal> {
    let version = value_string_at_path(source_artifact, &["version"]).ok_or_else(|| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply source artifact is missing a version",
            json!({
                "stage": "apply",
                "field": "version",
                "expected_versions": [CANON_ENTITY_SOLVE_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1],
                "writes_performed": false
            }),
            Some(
                "Use a self-hashed canon_entity_solve.v1 or canon_entity_run.v1 artifact"
                    .to_string(),
            ),
        )
    })?;

    if let Some(contract) = entity_artifact_v1_contract_for_legacy_version(&version) {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply refuses legacy entity result artifacts",
            json!({
                "stage": "apply",
                "reason": "legacy_entity_result_version",
                "actual_version": version,
                "expected_versions": [CANON_ENTITY_SOLVE_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1],
                "legacy_stage": contract.stage.as_str(),
                "legacy_versions": contract.legacy_versions,
                "writes_performed": false
            }),
            Some(
                "Re-run the entity pipeline to produce solve.v1 or run.v1, then rerun apply"
                    .to_string(),
            ),
        ));
    }

    let contract = entity_artifact_v1_contract_for_version(&version).ok_or_else(|| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply source artifact version is not registered",
            json!({
                "stage": "apply",
                "actual_version": version,
                "expected_versions": [CANON_ENTITY_SOLVE_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1],
                "writes_performed": false
            }),
            Some(
                "Use a self-hashed canon_entity_solve.v1 or canon_entity_run.v1 artifact"
                    .to_string(),
            ),
        )
    })?;
    if !matches!(
        contract.stage,
        EntityArtifactStageV1::Solve | EntityArtifactStageV1::Run
    ) {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply source artifact must be a solve.v1 or run.v1 artifact",
            json!({
                "stage": "apply",
                "reason": "wrong_entity_artifact_stage",
                "actual_stage": contract.stage.as_str(),
                "actual_version": version,
                "expected_stages": ["solve", "run"],
                "expected_versions": [CANON_ENTITY_SOLVE_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1],
                "writes_performed": false
            }),
            Some("Use a canon_entity_solve.v1 or canon_entity_run.v1 artifact".to_string()),
        ));
    }

    entity_v1_artifact_reference(source_artifact)
}

fn validate_apply_v1_registry_binding(
    source_artifact: &Value,
    actual_registry_id: &str,
    actual_registry_version: &str,
    actual_registry_snapshot_hash: &str,
) -> Result<(), Refusal> {
    for (field, actual) in [
        ("id", actual_registry_id),
        ("version", actual_registry_version),
        ("lookup_snapshot_hash", actual_registry_snapshot_hash),
    ] {
        let path = ["metadata", "registry_snapshot", field];
        let expected = value_string_at_path(source_artifact, &path).ok_or_else(|| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Apply source artifact is missing registry snapshot metadata",
                json!({
                    "stage": "apply",
                    "field": format!("metadata.registry_snapshot.{field}"),
                    "writes_performed": false
                }),
                Some(
                    "Use a self-hashed solve/run artifact with registry snapshot metadata"
                        .to_string(),
                ),
            )
        })?;
        if expected != actual {
            return Err(EntityRefusalKind::ArtifactContract.to_refusal(
                "Apply registry snapshot does not match the verified source artifact",
                json!({
                    "stage": "apply",
                    "field": format!("metadata.registry_snapshot.{field}"),
                    "expected": expected,
                    "actual": actual,
                    "writes_performed": false
                }),
                Some(
                    "Re-run apply with the registry snapshot used by the source artifact"
                        .to_string(),
                ),
            ));
        }
    }
    Ok(())
}

fn validate_apply_v1_input_binding(
    source_artifact: &Value,
    rows: &Path,
    lookup_column: &str,
    resolutions: &BTreeMap<String, ApplyCanonicalResolution>,
) -> Result<(), Refusal> {
    let expected_hash =
        value_string_at_path(source_artifact, &["metadata", "input", "content_hash"])
            .ok_or_else(|| apply_v1_input_binding_missing_refusal("metadata.input.content_hash"))?;
    let actual_hash = witness::hash_file(rows).map_err(|error| {
        io_budget_refusal("Failed to hash apply input rows", rows, error.to_string())
    })?;
    if expected_hash != actual_hash {
        return Err(apply_v1_input_binding_refusal(
            "metadata.input.content_hash",
            json!(expected_hash),
            json!(actual_hash),
        ));
    }

    let expected_row_count =
        value_u64_at_path(source_artifact, &["metadata", "input", "row_count"])
            .ok_or_else(|| apply_v1_input_binding_missing_refusal("metadata.input.row_count"))?;
    let format = apply_input_format(rows)?;
    let inspection = inspect_apply_input(rows, lookup_column, resolutions, format)?;
    if expected_row_count != inspection.row_count {
        return Err(apply_v1_input_binding_refusal(
            "metadata.input.row_count",
            json!(expected_row_count),
            json!(inspection.row_count),
        ));
    }

    Ok(())
}

fn apply_v1_input_binding_missing_refusal(field: &'static str) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Apply source artifact is missing input metadata",
        json!({
            "stage": "apply",
            "field": field,
            "writes_performed": false
        }),
        Some("Use a solve.v1 or run.v1 artifact with metadata.input bindings".to_string()),
    )
}

fn apply_v1_input_binding_refusal(field: &'static str, expected: Value, actual: Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Apply input rows do not match the verified source artifact",
        json!({
            "stage": "apply",
            "field": field,
            "expected": expected,
            "actual": actual,
            "writes_performed": false
        }),
        Some("Rerun apply with the row corpus used by the verified solve/run artifact".to_string()),
    )
}

fn apply_v1_safety_check(source_artifact: &Value) -> ApplySafetyCheck {
    ApplySafetyCheck {
        expected_profile_id: value_string_at_path(source_artifact, &["metadata", "profile", "id"]),
        expected_identity_semantics: value_string_at_path(
            source_artifact,
            &["metadata", "profile", "identity_semantics"],
        ),
        expected_registry_snapshot_hash: value_string_at_path(
            source_artifact,
            &["metadata", "registry_snapshot", "lookup_snapshot_hash"],
        ),
        expected_sidecar_snapshot_hash: value_string_at_path(
            source_artifact,
            &["metadata", "registry_snapshot", "sidecar_snapshot_hash"],
        ),
        ..ApplySafetyCheck::default()
    }
}

fn validate_apply_v1_safety_material(safety: &ApplySafetyCheck) -> Result<(), Refusal> {
    for (field, expected, actual) in [
        (
            "profile_id",
            safety.expected_profile_id.as_deref(),
            safety.actual_profile_id.as_deref(),
        ),
        (
            "identity_semantics",
            safety.expected_identity_semantics.as_deref(),
            safety.actual_identity_semantics.as_deref(),
        ),
    ] {
        if let (Some(expected), None) = (expected, actual) {
            return Err(EntityRefusalKind::ArtifactContract.to_refusal(
                "Apply registry metadata is incomplete for the verified v1 source artifact",
                json!({
                    "stage": "apply",
                    "field": field,
                    "expected": expected,
                    "actual": Value::Null,
                    "writes_performed": false
                }),
                Some("Use a registry with matching entity_profile metadata".to_string()),
            ));
        }
    }
    Ok(())
}

fn build_apply_v1_artifact(
    source_artifact: &Value,
    source_reference: EntityArtifactReferenceV1,
    data: ApplyStreamingRunData,
) -> Result<Value, Refusal> {
    let source_version = source_reference.version.clone();
    let source_content_hash = source_reference.content_hash.clone();
    let metadata = entity_v1_lifecycle_metadata_from_source(
        source_artifact,
        EntityArtifactStageV1::Apply,
        vec![source_reference],
    )?;
    let summary = EntityDeterministicSummary {
        counts: data.summary.clone(),
        labels: BTreeMap::from([
            ("lookup_column".to_string(), data.lookup_column.clone()),
            ("registry_id".to_string(), data.registry.id.clone()),
            (
                "registry_version".to_string(),
                data.registry.version.clone(),
            ),
            ("source_artifact_hash".to_string(), source_content_hash),
            ("source_artifact_version".to_string(), source_version),
            (
                "stage".to_string(),
                EntityArtifactStageV1::Apply.as_str().to_string(),
            ),
        ]),
    };
    let mut artifact = json!({
        "version": CANON_ENTITY_APPLY_VERSION_V1,
        "artifact_content_hash": "",
        "metadata": metadata,
        "summary": summary,
        "registry": data.registry,
        "registry_snapshot_hash": data.registry_snapshot_hash,
        "output_content_hash": data.output_content_hash,
        "streaming": data.streaming,
        "output_path": data.output_path
    });
    finalize_entity_v1_self_hash(&mut artifact)?;
    Ok(artifact)
}

fn value_string_at_path(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn value_u64_at_path(value: &Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_u64()
}

pub fn run_sec10d_org_apply_streaming(
    request: Sec10dOrgApplyStreamRequest<'_>,
) -> Result<ApplyRunArtifact, Refusal> {
    let format = apply_input_format(request.rows)?;
    if !matches!(format, ApplyInputFormat::Jsonl) {
        return Err(EntityRefusalKind::InputContract.to_refusal(
            "sec10d org apply output requires JSONL or NDJSON input rows",
            json!({
                "stage": "apply",
                "expected": "jsonl",
                "actual": format!("{:?}", entity_stream_format(format)).to_ascii_lowercase(),
                "writes_performed": false
            }),
            Some("Re-export org_mentions as JSONL, then rerun canon entity apply".to_string()),
        ));
    }

    let generic_resolutions = BTreeMap::new();
    validate_apply_safety(&ApplyStreamRequest {
        rows: request.rows,
        output: request.output,
        lookup_column: request.lookup_column,
        registry: request.registry.clone(),
        resolutions: &generic_resolutions,
        safety: request.safety.clone(),
        require_full_resolution: request.require_full_resolution,
        target_rows_per_chunk: request.target_rows_per_chunk,
    })?;

    let inspection = inspect_sec10d_org_jsonl(
        request.rows,
        request.lookup_column,
        request.field_name_column,
        request.resolutions,
    )?;
    if request.require_full_resolution && inspection.unresolved > 0 {
        return Err(apply_unresolved_refusal(
            &ApplyStreamRequest {
                rows: request.rows,
                output: request.output,
                lookup_column: request.lookup_column,
                registry: request.registry.clone(),
                resolutions: &generic_resolutions,
                safety: request.safety.clone(),
                require_full_resolution: request.require_full_resolution,
                target_rows_per_chunk: request.target_rows_per_chunk,
            },
            &inspection,
        ));
    }

    let byte_count = fs::metadata(request.rows)
        .map_err(|error| {
            io_budget_refusal(
                "Failed to inspect apply input rows",
                request.rows,
                error.to_string(),
            )
        })?
        .len();
    let content_hash = witness::hash_file(request.rows).map_err(|error| {
        io_budget_refusal(
            "Failed to hash apply input rows",
            request.rows,
            error.to_string(),
        )
    })?;
    let input = EntityStreamInput::new(
        EntityStreamStage::Apply,
        EntityStreamFormat::Jsonl,
        request.rows.display().to_string(),
        content_hash,
        inspection.row_count,
        byte_count,
    );
    let chunks = deterministic_chunk_metadata(&input, request.target_rows_per_chunk)?;
    let telemetry = stream_telemetry(&input, &chunks);
    let write_result = write_sec10d_org_jsonl_output(&request, &chunks)?;
    let output_content_hash = hash_apply_output(request.output)?;

    let mut artifact = ApplyRunArtifact {
        version: CANON_ENTITY_APPLY_VERSION.to_string(),
        artifact_content_hash: String::new(),
        registry: request.registry.clone(),
        registry_snapshot_hash: request.safety.actual_registry_snapshot_hash.clone(),
        output_content_hash,
        summary: BTreeMap::from([
            ("rows".to_string(), write_result.row_count),
            ("resolved".to_string(), write_result.resolved),
            ("unresolved".to_string(), write_result.unresolved),
        ]),
        streaming: ApplyStreamingDiagnostics {
            input,
            chunks,
            telemetry,
            provenance_samples: write_result.provenance_samples,
        },
        output_path: request.output.display().to_string(),
    };
    artifact.artifact_content_hash = hash_apply_artifact_without_self(&artifact)?;
    Ok(artifact)
}

fn inspect_apply_input(
    rows: &Path,
    lookup_column: &str,
    resolutions: &BTreeMap<String, ApplyCanonicalResolution>,
    format: ApplyInputFormat,
) -> Result<ApplyInputInspection, Refusal> {
    match format {
        ApplyInputFormat::Csv(delimiter) => {
            inspect_apply_csv(rows, lookup_column, resolutions, delimiter)
        }
        ApplyInputFormat::Jsonl => inspect_apply_jsonl(rows, lookup_column, resolutions),
    }
}

fn inspect_sec10d_org_jsonl(
    rows: &Path,
    lookup_column: &str,
    field_name_column: &str,
    resolutions: &BTreeMap<String, Sec10dOrgApplyResolution>,
) -> Result<ApplyInputInspection, Refusal> {
    let mut reader = open_apply_reader(rows)?;
    let mut line = Vec::new();
    let mut inspection = ApplyInputInspection {
        row_count: 0,
        resolved: 0,
        unresolved: 0,
    };

    loop {
        line.clear();
        let bytes_read = reader.read_until(b'\n', &mut line).map_err(|error| {
            input_contract_refusal(
                "Failed to read apply JSONL row",
                inspection.row_count + 1,
                lookup_column,
                error.to_string(),
            )
        })?;
        if bytes_read == 0 {
            break;
        }
        if blank_line(&line) {
            continue;
        }

        let row_number = inspection.row_count + 1;
        let object = parse_json_object(&line, row_number)?;
        let lookup_value = json_lookup_value(&object, lookup_column, row_number)?;
        let field_name = json_lookup_value(&object, field_name_column, row_number)?;
        validate_no_sec10d_org_json_fields(&object, row_number)?;
        sec10d_org_field_prefix(&field_name, row_number)?;

        inspection.row_count += 1;
        if resolutions.contains_key(lookup_value.as_str()) {
            inspection.resolved += 1;
        } else {
            inspection.unresolved += 1;
        }
    }

    Ok(inspection)
}

fn validate_apply_safety(request: &ApplyStreamRequest<'_>) -> Result<(), Refusal> {
    let safety = &request.safety;
    if let (Some(expected), Some(actual)) = (
        safety.expected_profile_id.as_deref(),
        safety.actual_profile_id.as_deref(),
    ) && expected != actual
    {
        return Err(apply_profile_refusal(
            request,
            "profile_id",
            Some(expected),
            Some(actual),
        ));
    }
    if let (Some(expected), Some(actual)) = (
        safety.expected_identity_semantics.as_deref(),
        safety.actual_identity_semantics.as_deref(),
    ) && expected != actual
    {
        return Err(apply_profile_refusal(
            request,
            "identity_semantics",
            Some(expected),
            Some(actual),
        ));
    }

    if let (Some(expected), Some(actual)) = (
        safety.expected_registry_snapshot_hash.as_deref(),
        safety.actual_registry_snapshot_hash.as_deref(),
    ) && expected != actual
    {
        return Err(apply_registry_snapshot_refusal(request, expected, actual));
    }

    match (
        safety.expected_sidecar_artifact_version.as_deref(),
        safety.actual_sidecar_artifact_version.as_deref(),
    ) {
        (Some(expected), Some(actual)) if expected != actual => {
            return Err(apply_sidecar_refusal(
                request,
                "sidecar_artifact_version",
                Some(expected),
                Some(actual),
            ));
        }
        (Some(expected), None) => {
            return Err(apply_sidecar_refusal(
                request,
                "sidecar_artifact_version",
                Some(expected),
                None,
            ));
        }
        (None, Some(actual)) => {
            return Err(apply_sidecar_refusal(
                request,
                "sidecar_artifact_version",
                None,
                Some(actual),
            ));
        }
        _ => {}
    }

    match (
        safety.expected_sidecar_snapshot_hash.as_deref(),
        safety.actual_sidecar_snapshot_hash.as_deref(),
    ) {
        (Some(expected), Some(actual)) if expected != actual => {
            return Err(apply_sidecar_refusal(
                request,
                "sidecar_snapshot_hash",
                Some(expected),
                Some(actual),
            ));
        }
        (Some(expected), None) => {
            return Err(apply_sidecar_refusal(
                request,
                "sidecar_snapshot_hash",
                Some(expected),
                None,
            ));
        }
        (None, Some(actual)) => {
            return Err(apply_sidecar_refusal(
                request,
                "sidecar_snapshot_hash",
                None,
                Some(actual),
            ));
        }
        _ => {}
    }

    Ok(())
}

fn inspect_apply_csv(
    rows: &Path,
    lookup_column: &str,
    resolutions: &BTreeMap<String, ApplyCanonicalResolution>,
    delimiter: u8,
) -> Result<ApplyInputInspection, Refusal> {
    let mut reader = open_apply_reader(rows)?;
    let mut line = Vec::new();
    let header_len = reader.read_until(b'\n', &mut line).map_err(|error| {
        input_contract_refusal(
            "Failed to read apply CSV headers",
            0,
            lookup_column,
            error.to_string(),
        )
    })?;
    if header_len == 0 {
        return Err(input_contract_refusal(
            "Apply CSV input must include headers",
            0,
            lookup_column,
            "empty input".to_string(),
        ));
    }
    let headers = parse_csv_record(&line, delimiter, 0, "headers")?;
    let lookup_index = csv_lookup_index(&headers, lookup_column)?;
    validate_no_canonical_csv_headers(&headers)?;

    let mut row_count = 0u64;
    let mut resolved = 0u64;
    let mut unresolved = 0u64;
    loop {
        line.clear();
        let bytes_read = reader.read_until(b'\n', &mut line).map_err(|error| {
            input_contract_refusal(
                "Failed to read apply CSV row",
                row_count + 1,
                lookup_column,
                error.to_string(),
            )
        })?;
        if bytes_read == 0 {
            break;
        }
        if blank_line(&line) {
            continue;
        }
        let row_number = row_count + 1;
        let record = parse_csv_record(&line, delimiter, row_number, "row")?;
        let lookup_value = ascii_trim(record.get(lookup_index).unwrap_or(""));
        if resolutions.contains_key(lookup_value) {
            resolved += 1;
        } else {
            unresolved += 1;
        }
        row_count += 1;
    }

    Ok(ApplyInputInspection {
        row_count,
        resolved,
        unresolved,
    })
}

fn inspect_apply_jsonl(
    rows: &Path,
    lookup_column: &str,
    resolutions: &BTreeMap<String, ApplyCanonicalResolution>,
) -> Result<ApplyInputInspection, Refusal> {
    let mut reader = open_apply_reader(rows)?;
    let mut line = Vec::new();
    let mut row_count = 0u64;
    let mut resolved = 0u64;
    let mut unresolved = 0u64;

    loop {
        line.clear();
        let bytes_read = reader.read_until(b'\n', &mut line).map_err(|error| {
            input_contract_refusal(
                "Failed to read apply JSONL row",
                row_count + 1,
                lookup_column,
                error.to_string(),
            )
        })?;
        if bytes_read == 0 {
            break;
        }
        if blank_line(&line) {
            continue;
        }
        let row_number = row_count + 1;
        let object = parse_json_object(&line, row_number)?;
        let lookup_value = json_lookup_value(&object, lookup_column, row_number)?;
        if resolutions.contains_key(lookup_value.as_str()) {
            resolved += 1;
        } else {
            unresolved += 1;
        }
        validate_no_canonical_json_fields(&object, row_number)?;
        row_count += 1;
    }

    Ok(ApplyInputInspection {
        row_count,
        resolved,
        unresolved,
    })
}

fn write_apply_output(
    request: &ApplyStreamRequest<'_>,
    format: ApplyInputFormat,
    chunks: &[EntityStreamChunkMetadata],
    output: &Path,
) -> Result<ApplyWriteResult, Refusal> {
    write_apply_output_atomic(output, |writer| match format {
        ApplyInputFormat::Csv(delimiter) => write_apply_csv(
            request.rows,
            request.lookup_column,
            &request.registry,
            request.resolutions,
            delimiter,
            chunks,
            writer,
        ),
        ApplyInputFormat::Jsonl => write_apply_jsonl(
            request.rows,
            request.lookup_column,
            &request.registry,
            request.resolutions,
            chunks,
            writer,
        ),
    })
}

fn publish_staged_apply_output(staged_output: &Path, output: &Path) -> Result<(), Refusal> {
    fs::rename(staged_output, output).map_err(|error| {
        io_budget_refusal(
            "Failed to install staged apply output atomically",
            output,
            error.to_string(),
        )
    })
}

fn cleanup_owned_staged_apply_output(staged_output: &Path) {
    let _ = fs::remove_file(staged_output);
}

fn write_apply_csv<W: Write>(
    rows: &Path,
    lookup_column: &str,
    registry: &ApplyRegistryReference,
    resolutions: &BTreeMap<String, ApplyCanonicalResolution>,
    delimiter: u8,
    chunks: &[EntityStreamChunkMetadata],
    writer: &mut W,
) -> Result<ApplyWriteResult, Refusal> {
    let mut reader = open_apply_reader(rows)?;
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line).map_err(|error| {
        input_contract_refusal(
            "Failed to read apply CSV headers",
            0,
            lookup_column,
            error.to_string(),
        )
    })?;
    let headers = parse_csv_record(&line, delimiter, 0, "headers")?;
    let lookup_index = csv_lookup_index(&headers, lookup_column)?;
    validate_no_canonical_csv_headers(&headers)?;
    write_csv_line_with_appended_fields(
        writer,
        &line,
        delimiter,
        &apply_canonical_header_fields(),
    )?;

    let mut result = ApplyWriteResult {
        row_count: 0,
        resolved: 0,
        unresolved: 0,
        provenance_samples: Vec::new(),
    };
    let mut byte_offset =
        u64::try_from(line.len()).expect("header line length fits u64 for provenance");

    loop {
        line.clear();
        let bytes_read = reader.read_until(b'\n', &mut line).map_err(|error| {
            input_contract_refusal(
                "Failed to read apply CSV row",
                result.row_count + 1,
                lookup_column,
                error.to_string(),
            )
        })?;
        if bytes_read == 0 {
            break;
        }
        if blank_line(&line) {
            writer.write_all(&line).map_err(|error| {
                io_budget_refusal("Failed to write apply output row", rows, error.to_string())
            })?;
            byte_offset += u64::try_from(line.len()).expect("line length fits u64");
            continue;
        }

        let row_number = result.row_count + 1;
        let record = parse_csv_record(&line, delimiter, row_number, "row")?;
        let lookup_value = ascii_trim(record.get(lookup_index).unwrap_or(""));
        let resolution = resolutions.get(lookup_value);
        let fields = csv_canonical_fields(registry, resolution);
        write_csv_line_with_appended_fields(writer, &line, delimiter, &fields)?;
        update_apply_write_result(
            &mut result,
            resolution.is_some(),
            lookup_value,
            byte_offset,
            u64::try_from(line.len()).expect("line length fits u64"),
            chunks,
        );
        byte_offset += u64::try_from(line.len()).expect("line length fits u64");
    }

    Ok(result)
}

fn write_apply_jsonl<W: Write>(
    rows: &Path,
    lookup_column: &str,
    registry: &ApplyRegistryReference,
    resolutions: &BTreeMap<String, ApplyCanonicalResolution>,
    chunks: &[EntityStreamChunkMetadata],
    writer: &mut W,
) -> Result<ApplyWriteResult, Refusal> {
    let mut reader = open_apply_reader(rows)?;
    let mut line = Vec::new();
    let mut result = ApplyWriteResult {
        row_count: 0,
        resolved: 0,
        unresolved: 0,
        provenance_samples: Vec::new(),
    };
    let mut byte_offset = 0u64;

    loop {
        line.clear();
        let bytes_read = reader.read_until(b'\n', &mut line).map_err(|error| {
            input_contract_refusal(
                "Failed to read apply JSONL row",
                result.row_count + 1,
                lookup_column,
                error.to_string(),
            )
        })?;
        if bytes_read == 0 {
            break;
        }
        if blank_line(&line) {
            writer.write_all(&line).map_err(|error| {
                io_budget_refusal("Failed to write apply output row", rows, error.to_string())
            })?;
            byte_offset += u64::try_from(line.len()).expect("line length fits u64");
            continue;
        }

        let row_number = result.row_count + 1;
        let object = parse_json_object(&line, row_number)?;
        let lookup_value = json_lookup_value(&object, lookup_column, row_number)?;
        validate_no_canonical_json_fields(&object, row_number)?;
        let resolution = resolutions.get(lookup_value.as_str());
        let appended =
            json_line_with_appended_fields(&line, registry, resolution, object.is_empty())?;
        writer.write_all(&appended).map_err(|error| {
            io_budget_refusal("Failed to write apply output row", rows, error.to_string())
        })?;
        update_apply_write_result(
            &mut result,
            resolution.is_some(),
            &lookup_value,
            byte_offset,
            u64::try_from(line.len()).expect("line length fits u64"),
            chunks,
        );
        byte_offset += u64::try_from(line.len()).expect("line length fits u64");
    }

    Ok(result)
}

fn write_sec10d_org_jsonl_output(
    request: &Sec10dOrgApplyStreamRequest<'_>,
    chunks: &[EntityStreamChunkMetadata],
) -> Result<ApplyWriteResult, Refusal> {
    write_apply_output_atomic(request.output, |writer| {
        let mut reader = open_apply_reader(request.rows)?;
        let mut line = Vec::new();
        let mut result = ApplyWriteResult {
            row_count: 0,
            resolved: 0,
            unresolved: 0,
            provenance_samples: Vec::new(),
        };
        let mut byte_offset = 0u64;

        loop {
            line.clear();
            let bytes_read = reader.read_until(b'\n', &mut line).map_err(|error| {
                input_contract_refusal(
                    "Failed to read apply JSONL row",
                    result.row_count + 1,
                    request.lookup_column,
                    error.to_string(),
                )
            })?;
            if bytes_read == 0 {
                break;
            }
            if blank_line(&line) {
                writer.write_all(&line).map_err(|error| {
                    io_budget_refusal(
                        "Failed to write apply output row",
                        request.rows,
                        error.to_string(),
                    )
                })?;
                byte_offset += u64::try_from(line.len()).expect("line length fits u64");
                continue;
            }

            let row_number = result.row_count + 1;
            let object = parse_json_object(&line, row_number)?;
            let lookup_value = json_lookup_value(&object, request.lookup_column, row_number)?;
            let field_name = json_lookup_value(&object, request.field_name_column, row_number)?;
            validate_no_sec10d_org_json_fields(&object, row_number)?;
            let prefix = sec10d_org_field_prefix(&field_name, row_number)?;
            let resolution = request.resolutions.get(lookup_value.as_str());
            let appended = json_line_with_appended_sec10d_org_fields(
                &line,
                &prefix,
                &request.registry,
                resolution,
                object.is_empty(),
            )?;
            writer.write_all(&appended).map_err(|error| {
                io_budget_refusal(
                    "Failed to write apply output row",
                    request.rows,
                    error.to_string(),
                )
            })?;
            update_apply_write_result(
                &mut result,
                resolution.is_some(),
                &lookup_value,
                byte_offset,
                u64::try_from(line.len()).expect("line length fits u64"),
                chunks,
            );
            byte_offset += u64::try_from(line.len()).expect("line length fits u64");
        }

        Ok(result)
    })
}

fn update_apply_write_result(
    result: &mut ApplyWriteResult,
    resolved: bool,
    source_row_id: &str,
    byte_start: u64,
    byte_len: u64,
    chunks: &[EntityStreamChunkMetadata],
) {
    let row_ordinal = result.row_count;
    result.row_count += 1;
    if resolved {
        result.resolved += 1;
    } else {
        result.unresolved += 1;
    }

    if result.provenance_samples.len() < MAX_APPLY_PROVENANCE_SAMPLES {
        let chunk = chunk_for_row(chunks, row_ordinal);
        result
            .provenance_samples
            .push(EntityStreamRowProvenance::new(
                EntityStreamStage::Apply,
                chunk.map(|chunk| chunk.chunk_index).unwrap_or_default(),
                row_ordinal,
                Some(source_row_id.to_string()),
                byte_start,
                byte_len,
            ));
    }
}

fn chunk_for_row(
    chunks: &[EntityStreamChunkMetadata],
    row_ordinal: u64,
) -> Option<&EntityStreamChunkMetadata> {
    chunks
        .iter()
        .find(|chunk| {
            row_ordinal >= chunk.first_row_ordinal && row_ordinal < chunk.row_end_exclusive()
        })
        .or_else(|| chunks.last())
}

fn apply_canonical_header_fields() -> Vec<String> {
    APPLY_CANONICAL_FIELDS
        .iter()
        .map(|field| (*field).to_string())
        .collect()
}

fn csv_canonical_fields(
    registry: &ApplyRegistryReference,
    resolution: Option<&ApplyCanonicalResolution>,
) -> Vec<String> {
    match resolution {
        Some(resolution) => vec![
            resolution.canonical_id.clone(),
            resolution.canonical_type.clone(),
            "resolved".to_string(),
            registry.id.clone(),
            registry.version.clone(),
            resolution.rule_id.clone(),
        ],
        None => vec![
            String::new(),
            String::new(),
            "unresolved".to_string(),
            registry.id.clone(),
            registry.version.clone(),
            String::new(),
        ],
    }
}

fn write_csv_line_with_appended_fields<W: Write>(
    writer: &mut W,
    line: &[u8],
    delimiter: u8,
    fields: &[String],
) -> Result<(), Refusal> {
    let ending = line_ending(line);
    writer
        .write_all(&line[..ending.body_len])
        .map_err(|error| {
            EntityRefusalKind::IoBudget.to_refusal(
                "Failed to write apply output row",
                json!({ "error": error.to_string() }),
                None,
            )
        })?;
    writer.write_all(&[delimiter]).map_err(|error| {
        EntityRefusalKind::IoBudget.to_refusal(
            "Failed to write apply output delimiter",
            json!({ "error": error.to_string() }),
            None,
        )
    })?;
    let encoded = encode_csv_fields(fields, delimiter)?;
    writer.write_all(&encoded).map_err(|error| {
        EntityRefusalKind::IoBudget.to_refusal(
            "Failed to write apply output canonical fields",
            json!({ "error": error.to_string() }),
            None,
        )
    })?;
    writer
        .write_all(&line[ending.body_len..ending.body_len + ending.ending_len])
        .map_err(|error| {
            EntityRefusalKind::IoBudget.to_refusal(
                "Failed to write apply output line ending",
                json!({ "error": error.to_string() }),
                None,
            )
        })?;
    Ok(())
}

fn encode_csv_fields(fields: &[String], delimiter: u8) -> Result<Vec<u8>, Refusal> {
    let mut writer = WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    writer.write_record(fields).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to encode apply canonical CSV fields",
            json!({ "error": error.to_string() }),
            None,
        )
    })?;
    let mut bytes = writer.into_inner().map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to finalize apply canonical CSV fields",
            json!({ "error": error.into_error().to_string() }),
            None,
        )
    })?;
    if bytes.ends_with(b"\n") {
        bytes.pop();
    }
    if bytes.ends_with(b"\r") {
        bytes.pop();
    }
    Ok(bytes)
}

fn json_line_with_appended_fields(
    line: &[u8],
    registry: &ApplyRegistryReference,
    resolution: Option<&ApplyCanonicalResolution>,
    object_is_empty: bool,
) -> Result<Vec<u8>, Refusal> {
    let (body_end, close_index) = json_object_close_index(line)?;
    let mut output = Vec::with_capacity(line.len() + 96);
    output.extend_from_slice(&line[..close_index]);
    if !object_is_empty {
        output.extend_from_slice(b",");
    }
    let fields = json_canonical_fields(registry, resolution)?;
    output.extend_from_slice(fields.as_bytes());
    output.extend_from_slice(&line[close_index..body_end]);
    output.extend_from_slice(&line[body_end..]);
    Ok(output)
}

fn json_line_with_appended_sec10d_org_fields(
    line: &[u8],
    prefix: &str,
    registry: &ApplyRegistryReference,
    resolution: Option<&Sec10dOrgApplyResolution>,
    object_is_empty: bool,
) -> Result<Vec<u8>, Refusal> {
    let (body_end, close_index) = json_object_close_index(line)?;
    let mut output = Vec::with_capacity(line.len() + 192);
    output.extend_from_slice(&line[..close_index]);
    if !object_is_empty {
        output.extend_from_slice(b",");
    }
    let fields = json_sec10d_org_fields(prefix, registry, resolution)?;
    output.extend_from_slice(fields.as_bytes());
    output.extend_from_slice(&line[close_index..body_end]);
    output.extend_from_slice(&line[body_end..]);
    Ok(output)
}

fn json_canonical_fields(
    registry: &ApplyRegistryReference,
    resolution: Option<&ApplyCanonicalResolution>,
) -> Result<String, Refusal> {
    let mut fields = Vec::with_capacity(APPLY_CANONICAL_FIELDS.len());
    match resolution {
        Some(resolution) => {
            fields.push(json_field("canonical_id", Some(&resolution.canonical_id))?);
            fields.push(json_field(
                "canonical_type",
                Some(&resolution.canonical_type),
            )?);
            fields.push(json_field("canonical_status", Some("resolved"))?);
            fields.push(json_field("canonical_registry_id", Some(&registry.id))?);
            fields.push(json_field(
                "canonical_registry_version",
                Some(&registry.version),
            )?);
            fields.push(json_field("canonical_rule_id", Some(&resolution.rule_id))?);
        }
        None => {
            fields.push(json_field("canonical_id", None)?);
            fields.push(json_field("canonical_type", None)?);
            fields.push(json_field("canonical_status", Some("unresolved"))?);
            fields.push(json_field("canonical_registry_id", Some(&registry.id))?);
            fields.push(json_field(
                "canonical_registry_version",
                Some(&registry.version),
            )?);
            fields.push(json_field("canonical_rule_id", None)?);
        }
    }
    Ok(fields.join(","))
}

fn json_sec10d_org_fields(
    prefix: &str,
    registry: &ApplyRegistryReference,
    resolution: Option<&Sec10dOrgApplyResolution>,
) -> Result<String, Refusal> {
    let mut fields = Vec::with_capacity(SEC10D_ORG_FIELD_SUFFIXES.len());
    match resolution {
        Some(resolution) => {
            fields.push(json_field(
                &format!("{prefix}_org_canon_id"),
                Some(&resolution.canonical_id),
            )?);
            fields.push(json_field(
                &format!("{prefix}_org_canonical_name"),
                Some(&resolution.canonical_name),
            )?);
            fields.push(json_field(
                &format!("{prefix}_org_resolution_status"),
                Some(&resolution.resolution_status),
            )?);
            fields.push(json_field(
                &format!("{prefix}_org_registry_id"),
                Some(&registry.id),
            )?);
            fields.push(json_field(
                &format!("{prefix}_org_registry_version"),
                Some(&registry.version),
            )?);
            fields.push(json_field(
                &format!("{prefix}_org_rule_id"),
                Some(&resolution.rule_id),
            )?);
        }
        None => {
            fields.push(json_field(&format!("{prefix}_org_canon_id"), None)?);
            fields.push(json_field(&format!("{prefix}_org_canonical_name"), None)?);
            fields.push(json_field(
                &format!("{prefix}_org_resolution_status"),
                Some("review_required"),
            )?);
            fields.push(json_field(
                &format!("{prefix}_org_registry_id"),
                Some(&registry.id),
            )?);
            fields.push(json_field(
                &format!("{prefix}_org_registry_version"),
                Some(&registry.version),
            )?);
            fields.push(json_field(&format!("{prefix}_org_rule_id"), None)?);
        }
    }
    Ok(fields.join(","))
}

fn sec10d_org_field_prefix(field_name: &str, row_number: u64) -> Result<String, Refusal> {
    let field_name = ascii_trim(field_name);
    let raw_prefix = field_name.strip_suffix("_name").unwrap_or(field_name);
    let mut prefix = String::with_capacity(raw_prefix.len());
    let mut previous_was_separator = false;

    for character in raw_prefix.chars() {
        if character.is_ascii_alphanumeric() {
            prefix.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if character == '_' || character == '-' || character.is_ascii_whitespace() {
            if !previous_was_separator && !prefix.is_empty() {
                prefix.push('_');
                previous_was_separator = true;
            }
        } else {
            return Err(input_contract_refusal(
                "Apply JSONL field_name cannot be converted to a Snowflake org field prefix",
                row_number,
                "field_name",
                field_name.to_string(),
            ));
        }
    }

    while prefix.ends_with('_') {
        prefix.pop();
    }
    if prefix.is_empty() {
        return Err(input_contract_refusal(
            "Apply JSONL field_name cannot be empty for sec10d org enrichment",
            row_number,
            "field_name",
            field_name.to_string(),
        ));
    }
    Ok(prefix)
}

fn validate_no_sec10d_org_json_fields(
    object: &Map<String, Value>,
    row_number: u64,
) -> Result<(), Refusal> {
    if let Some(field) = object.keys().find(|key| {
        SEC10D_ORG_FIELD_SUFFIXES
            .iter()
            .any(|suffix| key.ends_with(suffix))
    }) {
        return Err(input_contract_refusal(
            "Apply JSONL row already contains a sec10d org canonical field",
            row_number,
            field,
            "canonical field collision".to_string(),
        ));
    }
    Ok(())
}

fn json_field(key: &str, value: Option<&str>) -> Result<String, Refusal> {
    let key = serde_json::to_string(key).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to encode apply JSON field key",
            json!({ "error": error.to_string() }),
            None,
        )
    })?;
    let value = serde_json::to_string(&value).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to encode apply JSON field value",
            json!({ "error": error.to_string() }),
            None,
        )
    })?;
    Ok(format!("{key}:{value}"))
}

fn json_object_close_index(line: &[u8]) -> Result<(usize, usize), Refusal> {
    let ending = line_ending(line);
    let mut cursor = ending.body_len;
    while cursor > 0 && line[cursor - 1].is_ascii_whitespace() {
        cursor -= 1;
    }
    if cursor == 0 || line[cursor - 1] != b'}' {
        return Err(EntityRefusalKind::InputContract.to_refusal(
            "Apply JSONL rows must be JSON objects",
            json!({ "error": "row does not end with an object close" }),
            None,
        ));
    }
    Ok((ending.body_len, cursor - 1))
}

fn parse_csv_record(
    line: &[u8],
    delimiter: u8,
    row_number: u64,
    label: &str,
) -> Result<StringRecord, Refusal> {
    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(line);
    let mut records = reader.records();
    match records.next() {
        Some(Ok(record)) => Ok(record),
        Some(Err(error)) => Err(input_contract_refusal(
            format!("Failed to parse apply CSV {label}"),
            row_number,
            label,
            error.to_string(),
        )),
        None => Ok(StringRecord::new()),
    }
}

fn parse_json_object(line: &[u8], row_number: u64) -> Result<Map<String, Value>, Refusal> {
    let value = serde_json::from_slice::<Value>(line).map_err(|error| {
        input_contract_refusal(
            "Invalid apply JSONL row",
            row_number,
            "json",
            error.to_string(),
        )
    })?;
    let Value::Object(object) = value else {
        return Err(input_contract_refusal(
            "Apply JSONL rows must be JSON objects",
            row_number,
            "json",
            "non-object row".to_string(),
        ));
    };
    Ok(object)
}

fn json_lookup_value(
    object: &Map<String, Value>,
    lookup_column: &str,
    row_number: u64,
) -> Result<String, Refusal> {
    let Some(value) = object.get(lookup_column) else {
        return Err(input_contract_refusal(
            "Apply JSONL row is missing the lookup column",
            row_number,
            lookup_column,
            "missing field".to_string(),
        ));
    };
    match value {
        Value::String(text) => Ok(ascii_trim(text).to_string()),
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => Err(input_contract_refusal(
            "Apply JSONL lookup field must be scalar",
            row_number,
            lookup_column,
            "non-scalar field".to_string(),
        )),
    }
}

fn csv_lookup_index(headers: &StringRecord, lookup_column: &str) -> Result<usize, Refusal> {
    headers
        .iter()
        .position(|header| header == lookup_column)
        .ok_or_else(|| {
            EntityRefusalKind::InputContract.to_refusal(
                "Apply CSV input is missing the lookup column",
                json!({
                    "field": lookup_column,
                    "available": headers.iter().collect::<Vec<_>>(),
                }),
                Some(
                    "canon entity apply <ROWS> --registry <REGISTRY_DIR> --column <COLUMN>"
                        .to_string(),
                ),
            )
        })
}

fn validate_no_canonical_csv_headers(headers: &StringRecord) -> Result<(), Refusal> {
    for field in APPLY_CANONICAL_FIELDS {
        if headers.iter().any(|header| header == *field) {
            return Err(EntityRefusalKind::InputContract.to_refusal(
                "Apply output canonical field already exists in input",
                json!({ "field": field }),
                None,
            ));
        }
    }
    Ok(())
}

fn validate_no_canonical_json_fields(
    object: &Map<String, Value>,
    row_number: u64,
) -> Result<(), Refusal> {
    for field in APPLY_CANONICAL_FIELDS {
        if object.contains_key(*field) {
            return Err(input_contract_refusal(
                "Apply output canonical field already exists in input",
                row_number,
                field,
                "field conflict".to_string(),
            ));
        }
    }
    Ok(())
}

fn apply_input_format(path: &Path) -> Result<ApplyInputFormat, Refusal> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("csv") => Ok(ApplyInputFormat::Csv(b',')),
        Some("tsv") => Ok(ApplyInputFormat::Csv(b'\t')),
        Some("jsonl" | "ndjson") => Ok(ApplyInputFormat::Jsonl),
        _ => Err(EntityRefusalKind::InputContract.to_refusal(
            "Apply input must be CSV, TSV, JSONL, or NDJSON",
            json!({
                "path": path.display().to_string(),
                "supported_extensions": ["csv", "tsv", "jsonl", "ndjson"]
            }),
            None,
        )),
    }
}

fn entity_stream_format(format: ApplyInputFormat) -> EntityStreamFormat {
    match format {
        ApplyInputFormat::Csv(_) => EntityStreamFormat::Csv,
        ApplyInputFormat::Jsonl => EntityStreamFormat::Jsonl,
    }
}

fn open_apply_reader(path: &Path) -> Result<BufReader<File>, Refusal> {
    let file = File::open(path).map_err(|error| {
        EntityRefusalKind::InputContract.to_refusal(
            "Failed to read apply input rows",
            json!({ "path": path.display().to_string(), "error": error.to_string() }),
            None,
        )
    })?;
    Ok(BufReader::new(file))
}

fn blank_line(line: &[u8]) -> bool {
    line.iter().all(u8::is_ascii_whitespace)
}

fn line_ending(line: &[u8]) -> LineEnding {
    if line.ends_with(b"\r\n") {
        LineEnding {
            body_len: line.len() - 2,
            ending_len: 2,
        }
    } else if line.ends_with(b"\n") {
        LineEnding {
            body_len: line.len() - 1,
            ending_len: 1,
        }
    } else {
        LineEnding {
            body_len: line.len(),
            ending_len: 0,
        }
    }
}

fn hash_apply_artifact_without_self(artifact: &ApplyRunArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash apply artifact",
            json!({ "error": error.to_string() }),
            None,
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn hash_apply_output(output: &Path) -> Result<String, Refusal> {
    witness::hash_file(output).map_err(|error| {
        io_budget_refusal(
            "Failed to hash apply output rows",
            output,
            error.to_string(),
        )
    })
}

fn io_budget_refusal(message: &str, path: &Path, error: String) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        message,
        json!({ "path": path.display().to_string(), "error": error }),
        None,
    )
}

fn write_apply_output_atomic<F>(output: &Path, write: F) -> Result<ApplyWriteResult, Refusal>
where
    F: FnOnce(&mut File) -> Result<ApplyWriteResult, Refusal>,
{
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            io_budget_refusal(
                "Failed to create apply output directory",
                parent,
                error.to_string(),
            )
        })?;
    }
    cleanup_stale_apply_temp_siblings(output).map_err(|error| {
        io_budget_refusal(
            "Failed to clean stale apply output temp files",
            output,
            error.to_string(),
        )
    })?;
    let temp_path = apply_temp_sibling(output);
    let mut writer = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| {
            io_budget_refusal(
                "Failed to create temporary apply output",
                &temp_path,
                error.to_string(),
            )
        })?;

    let result = match write(&mut writer) {
        Ok(result) => result,
        Err(refusal) => {
            drop(writer);
            let _ = fs::remove_file(&temp_path);
            return Err(refusal);
        }
    };

    if let Err(error) = writer.flush() {
        drop(writer);
        let _ = fs::remove_file(&temp_path);
        return Err(io_budget_refusal(
            "Failed to flush temporary apply output",
            &temp_path,
            error.to_string(),
        ));
    }
    if let Err(error) = writer.sync_all() {
        drop(writer);
        let _ = fs::remove_file(&temp_path);
        return Err(io_budget_refusal(
            "Failed to sync temporary apply output",
            &temp_path,
            error.to_string(),
        ));
    }
    drop(writer);

    if let Err(error) = fs::rename(&temp_path, output) {
        let _ = fs::remove_file(&temp_path);
        return Err(io_budget_refusal(
            "Failed to install apply output atomically",
            output,
            error.to_string(),
        ));
    }

    Ok(result)
}

fn apply_resolutions_from_registry(
    registry: &crate::Registry,
) -> Result<BTreeMap<String, ApplyCanonicalResolution>, Refusal> {
    let conn = Connection::open_with_flags(
        &registry.db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply could not open the registry lookup index",
            json!({
                "stage": "apply",
                "registry_id": registry.meta.id.as_str(),
                "registry_version": registry.meta.version.as_str(),
                "db_path": registry.db_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Rebuild or repair the registry index, then rerun canon entity apply".to_string()),
        )
    })?;
    let mut stmt = conn
        .prepare(
            "SELECT input, canonical_id, canonical_type, rule_id
             FROM entries
             ORDER BY source_file ASC, entry_order ASC",
        )
        .map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Apply could not prepare the registry lookup query",
                json!({
                    "stage": "apply",
                    "registry_id": registry.meta.id.as_str(),
                    "registry_version": registry.meta.version.as_str(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some(
                    "Rebuild or repair the registry index, then rerun canon entity apply"
                        .to_string(),
                ),
            )
        })?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ApplyCanonicalResolution {
                    canonical_id: row.get::<_, String>(1)?,
                    canonical_type: row.get::<_, String>(2)?,
                    rule_id: row.get::<_, String>(3)?,
                },
            ))
        })
        .map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Apply could not read the registry lookup index",
                json!({
                    "stage": "apply",
                    "registry_id": registry.meta.id.as_str(),
                    "registry_version": registry.meta.version.as_str(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some(
                    "Rebuild or repair the registry index, then rerun canon entity apply"
                        .to_string(),
                ),
            )
        })?;

    let mut resolutions = BTreeMap::new();
    for row in rows {
        let (input, resolution) = row.map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Apply could not materialize a registry lookup row",
                json!({
                    "stage": "apply",
                    "registry_id": registry.meta.id.as_str(),
                    "registry_version": registry.meta.version.as_str(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some(
                    "Rebuild or repair the registry index, then rerun canon entity apply"
                        .to_string(),
                ),
            )
        })?;
        resolutions.entry(input).or_insert(resolution);
    }
    Ok(resolutions)
}

fn apply_registry_snapshot_hash(registry_dir: &Path) -> Result<String, Refusal> {
    let mut files = vec![registry_dir.join("registry.json")];
    files.extend(apply_mapping_file_paths(registry_dir)?);
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                EntityRefusalKind::ArtifactContract.to_refusal(
                    "Apply registry path is not valid UTF-8",
                    json!({
                        "stage": "apply",
                        "path": path.display().to_string(),
                        "writes_performed": false
                    }),
                    Some("Rename the registry file, then rerun canon entity apply".to_string()),
                )
            })?;
        let bytes = fs::read(&path).map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Apply could not hash the registry snapshot",
                json!({
                    "stage": "apply",
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some("Fix registry file permissions, then rerun canon entity apply".to_string()),
            )
        })?;
        hasher.update(file_name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
        hasher.update(&[0xff]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn apply_registry_profile_metadata(
    registry_dir: &Path,
) -> Result<(Option<String>, Option<String>), Refusal> {
    #[derive(Deserialize)]
    struct RegistryProfile {
        id: Option<String>,
        identity_semantics: Option<String>,
    }

    #[derive(Deserialize)]
    struct RegistryJson {
        #[serde(default)]
        entity_profile: Option<RegistryProfile>,
    }

    let registry_json_path = registry_dir.join("registry.json");
    let content = fs::read_to_string(&registry_json_path).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply could not read registry profile metadata",
            json!({
                "stage": "apply",
                "path": registry_json_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Fix registry.json, then rerun canon entity apply".to_string()),
        )
    })?;
    let registry_json = serde_json::from_str::<RegistryJson>(&content).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply could not parse registry profile metadata",
            json!({
                "stage": "apply",
                "path": registry_json_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Fix registry.json, then rerun canon entity apply".to_string()),
        )
    })?;
    Ok(registry_json
        .entity_profile
        .map(|profile| (profile.id, profile.identity_semantics))
        .unwrap_or((None, None)))
}

fn apply_registry_identity_metadata(registry_dir: &Path) -> Result<(String, String), Refusal> {
    #[derive(Deserialize)]
    struct RegistryJson {
        id: String,
        version: String,
    }

    let registry_json_path = registry_dir.join("registry.json");
    let content = fs::read_to_string(&registry_json_path).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply could not read registry identity metadata",
            json!({
                "stage": "apply",
                "path": registry_json_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Fix registry.json, then rerun canon entity apply".to_string()),
        )
    })?;
    let registry_json = serde_json::from_str::<RegistryJson>(&content).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply could not parse registry identity metadata",
            json!({
                "stage": "apply",
                "path": registry_json_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Fix registry.json, then rerun canon entity apply".to_string()),
        )
    })?;
    if registry_json.id.trim().is_empty() || registry_json.version.trim().is_empty() {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply registry identity metadata is incomplete",
            json!({
                "stage": "apply",
                "path": registry_json_path.display().to_string(),
                "writes_performed": false
            }),
            Some("Fix registry.json id/version, then rerun canon entity apply".to_string()),
        ));
    }
    Ok((registry_json.id, registry_json.version))
}

fn apply_mapping_file_paths(registry_dir: &Path) -> Result<Vec<PathBuf>, Refusal> {
    let entries = fs::read_dir(registry_dir).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Apply could not read the registry directory",
            json!({
                "stage": "apply",
                "registry": registry_dir.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Fix the registry directory, then rerun canon entity apply".to_string()),
        )
    })?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| {
                EntityRefusalKind::ArtifactContract.to_refusal(
                    "Apply could not inspect a registry directory entry",
                    json!({
                        "stage": "apply",
                        "registry": registry_dir.display().to_string(),
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                    Some("Fix the registry directory, then rerun canon entity apply".to_string()),
                )
            })?
            .path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
            && path.file_name().and_then(|name| name.to_str()) != Some("registry.json")
            && path.file_name().and_then(|name| name.to_str()) != Some("_build.json")
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn apply_temp_sibling(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("apply-output");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_file_name(format!(
        "{file_name}.canon-apply.{}.{}.tmp",
        std::process::id(),
        unique
    ))
}

fn cleanup_stale_apply_temp_siblings(path: &Path) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("apply-output");
    let prefix = format!("{file_name}.canon-apply.");
    let now = SystemTime::now();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let entry_path = entry.path();
        let Some(name) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".tmp") {
            continue;
        }
        let age = entry
            .metadata()?
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or_default();
        if age >= APPLY_TEMP_FILE_STALE_AFTER {
            let _ = fs::remove_file(&entry_path);
        }
    }
    Ok(())
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

fn input_contract_refusal(
    message: impl Into<String>,
    row_number: u64,
    field: &str,
    error: String,
) -> Refusal {
    EntityRefusalKind::InputContract.to_refusal(
        message,
        json!({
            "stage": "apply",
            "row_number": row_number,
            "field": field,
            "error": error,
            "writes_performed": false
        }),
        None,
    )
}

fn apply_registry_snapshot_refusal(
    request: &ApplyStreamRequest<'_>,
    expected: &str,
    actual: &str,
) -> Refusal {
    EntityRefusalKind::RegistrySnapshot.to_refusal(
        "Apply registry snapshot does not match the artifact being replayed",
        json!({
            "stage": "apply",
            "field": "registry_snapshot_hash",
            "expected_registry_snapshot_hash": expected,
            "actual_registry_snapshot_hash": actual,
            "registry_id": request.registry.id.as_str(),
            "registry_version": request.registry.version.as_str(),
            "output_path": request.output.display().to_string(),
            "writes_performed": false
        }),
        Some("Re-run apply with the registry snapshot used by promotion".to_string()),
    )
}

fn apply_sidecar_refusal(
    request: &ApplyStreamRequest<'_>,
    field: &'static str,
    expected: Option<&str>,
    actual: Option<&str>,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Apply sidecar artifact does not match the replay contract",
        json!({
            "stage": "apply",
            "field": field,
            "expected": expected,
            "actual": actual,
            "registry_id": request.registry.id.as_str(),
            "registry_version": request.registry.version.as_str(),
            "output_path": request.output.display().to_string(),
            "writes_performed": false
        }),
        Some("Use the sidecar artifact produced by the matching promotion run".to_string()),
    )
}

fn apply_profile_refusal(
    request: &ApplyStreamRequest<'_>,
    field: &'static str,
    expected: Option<&str>,
    actual: Option<&str>,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Apply profile metadata crossed the entity profile firewall",
        json!({
            "stage": "apply",
            "field": field,
            "expected": expected,
            "actual": actual,
            "registry_id": request.registry.id.as_str(),
            "registry_version": request.registry.version.as_str(),
            "output_path": request.output.display().to_string(),
            "writes_performed": false
        }),
        Some("Use artifacts and sidecars produced by the same entity profile".to_string()),
    )
}

fn apply_unresolved_refusal(
    request: &ApplyStreamRequest<'_>,
    inspection: &ApplyInputInspection,
) -> Refusal {
    EntityRefusalKind::ApplyUnresolved.to_refusal(
        "Apply was configured to require full resolution but unresolved rows remain",
        json!({
            "stage": "apply",
            "field": request.lookup_column,
            "rows": inspection.row_count,
            "resolved": inspection.resolved,
            "unresolved": inspection.unresolved,
            "registry_id": request.registry.id.as_str(),
            "registry_version": request.registry.version.as_str(),
            "output_path": request.output.display().to_string(),
            "writes_performed": false
        }),
        Some("Promote more aliases or rerun with partial output allowed".to_string()),
    )
}

pub fn default_apply_output_path(rows: &Path) -> PathBuf {
    let mut output = rows.to_path_buf();
    let extension = rows
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("out");
    output.set_extension(format!("canon.{extension}"));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RefusalCode;
    use std::fs;

    #[test]
    fn registry_backed_apply_replays_exact_aliases_and_ascii_trim_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        let rows = temp.path().join("rows.csv");
        let output = temp.path().join("rows.canon.csv");
        write_registry(&registry);
        fs::write(&rows, "tenant_name,amount\n Sears\t,10\nSears\u{00a0},20\n").expect("rows");

        let artifact = run_apply_streaming_from_registry(ApplyRegistryStreamRequest {
            rows: &rows,
            output: &output,
            lookup_column: "tenant_name",
            registry_dir: &registry,
            safety: ApplySafetyCheck::default(),
            require_full_resolution: false,
            target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
        })
        .expect("registry-backed apply succeeds");

        assert_eq!(artifact.registry.id, "cmbs-tenants");
        assert_eq!(artifact.registry.version, "1.0.0");
        assert_eq!(artifact.summary["rows"], 2);
        assert_eq!(artifact.summary["resolved"], 1);
        assert_eq!(artifact.summary["unresolved"], 1);
        assert_eq!(
            fs::read_to_string(&output).expect("apply output"),
            concat!(
                "tenant_name,amount,canonical_id,canonical_type,canonical_status,",
                "canonical_registry_id,canonical_registry_version,canonical_rule_id\n",
                " Sears\t,10,TNT-SEARS,tenant_label,resolved,cmbs-tenants,1.0.0,",
                "ENTITY_REVIEW_PROMOTE\n",
                "Sears\u{00a0},20,,,unresolved,cmbs-tenants,1.0.0,\n",
            )
        );
    }

    #[test]
    fn registry_backed_apply_populates_snapshot_hash_and_refuses_stale_registry_before_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        let rows = temp.path().join("rows.csv");
        let output = temp.path().join("rows.canon.csv");
        write_registry(&registry);
        fs::write(&rows, "tenant_name\nSears\n").expect("rows");
        fs::write(&output, "sentinel output\n").expect("sentinel");

        let refusal = run_apply_streaming_from_registry(ApplyRegistryStreamRequest {
            rows: &rows,
            output: &output,
            lookup_column: "tenant_name",
            registry_dir: &registry,
            safety: ApplySafetyCheck {
                expected_registry_snapshot_hash: Some("blake3:not-current".to_string()),
                ..ApplySafetyCheck::default()
            },
            require_full_resolution: true,
            target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
        })
        .expect_err("stale registry refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
        assert_eq!(refusal.detail["stage"], "apply");
        assert_eq!(
            refusal.detail["expected_registry_snapshot_hash"],
            "blake3:not-current"
        );
        assert!(
            refusal.detail["actual_registry_snapshot_hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("blake3:"))
        );
        assert_eq!(refusal.detail["writes_performed"], false);
        assert_eq!(
            fs::read_to_string(&output).expect("output after refusal"),
            "sentinel output\n"
        );
    }

    #[test]
    fn v1_apply_artifact_failure_preserves_existing_output_after_streaming() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        let rows = temp.path().join("rows.csv");
        let output = temp.path().join("rows.canon.csv");
        write_registry(&registry);
        fs::write(&rows, "tenant_name\nSears\n").expect("rows");
        fs::write(&output, "sentinel output\n").expect("sentinel");
        let registry_hash = apply_registry_snapshot_hash(&registry).expect("registry hash");
        let input_hash = witness::hash_file(&rows).expect("rows hash");
        let source = v1_apply_test_source_artifact(
            &registry_hash,
            &input_hash,
            1,
            temp.path().display().to_string().as_str(),
        );
        let final_output_path = output.display().to_string();
        let final_output_for_builder = output.clone();

        let refusal = run_apply_v1_from_registry_with_builder(
            ApplyV1ArtifactRequest {
                source_artifact: &source,
                rows: &rows,
                output: &output,
                lookup_column: "tenant_name",
                registry_dir: &registry,
                require_full_resolution: true,
                target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
            },
            move |_source, _source_reference, data| {
                assert_eq!(data.output_path, final_output_path);
                assert!(
                    data.output_content_hash.starts_with("blake3:"),
                    "staged bytes must be hashed before final publication"
                );
                assert_eq!(
                    fs::read_to_string(&final_output_for_builder).expect("final output in builder"),
                    "sentinel output\n",
                    "final output must remain untouched before artifact success"
                );
                Err(EntityRefusalKind::ArtifactContract.to_refusal(
                    "forced post-stream apply.v1 artifact failure",
                    json!({
                        "stage": "apply",
                        "reason": "forced_post_stream_artifact_failure",
                        "writes_performed": false
                    }),
                    None,
                ))
            },
        )
        .expect_err("forced artifact failure refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
        assert_eq!(
            refusal.detail["reason"],
            "forced_post_stream_artifact_failure"
        );
        assert_eq!(
            fs::read_to_string(&output).expect("output after refusal"),
            "sentinel output\n"
        );
        let leaked_temp = fs::read_dir(temp.path())
            .expect("tempdir entries")
            .map(|entry| entry.expect("entry").file_name())
            .any(|name| {
                name.to_str()
                    .is_some_and(|name| name.contains(".canon-apply."))
            });
        assert!(!leaked_temp, "owned staged apply temp should be removed");
    }

    #[test]
    fn v1_apply_refuses_input_hash_mismatch_before_staging_output() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        let rows = temp.path().join("rows.csv");
        let output = temp.path().join("rows.canon.csv");
        write_registry(&registry);
        fs::write(&rows, "tenant_name\nSears\n").expect("rows");
        fs::write(&output, "sentinel output\n").expect("sentinel");
        let registry_hash = apply_registry_snapshot_hash(&registry).expect("registry hash");
        let source = v1_apply_test_source_artifact(
            &registry_hash,
            "blake3:not-the-actual-rows",
            1,
            temp.path().display().to_string().as_str(),
        );

        let refusal = run_apply_v1_from_registry(ApplyV1ArtifactRequest {
            source_artifact: &source,
            rows: &rows,
            output: &output,
            lookup_column: "tenant_name",
            registry_dir: &registry,
            require_full_resolution: true,
            target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
        })
        .expect_err("input hash mismatch refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
        assert_eq!(refusal.detail["stage"], "apply");
        assert_eq!(refusal.detail["field"], "metadata.input.content_hash");
        assert_eq!(refusal.detail["expected"], "blake3:not-the-actual-rows");
        assert!(
            refusal.detail["actual"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("blake3:"))
        );
        assert_eq!(refusal.detail["writes_performed"], false);
        assert_eq!(
            fs::read_to_string(&output).expect("output after refusal"),
            "sentinel output\n"
        );
        assert_no_apply_temp_sibling(temp.path());
    }

    #[test]
    fn v1_apply_refuses_input_row_count_mismatch_before_staging_output() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        let rows = temp.path().join("rows.csv");
        let output = temp.path().join("rows.canon.csv");
        write_registry(&registry);
        fs::write(&rows, "tenant_name\nSears\n").expect("rows");
        fs::write(&output, "sentinel output\n").expect("sentinel");
        let registry_hash = apply_registry_snapshot_hash(&registry).expect("registry hash");
        let input_hash = witness::hash_file(&rows).expect("rows hash");
        let source = v1_apply_test_source_artifact(
            &registry_hash,
            &input_hash,
            2,
            temp.path().display().to_string().as_str(),
        );

        let refusal = run_apply_v1_from_registry(ApplyV1ArtifactRequest {
            source_artifact: &source,
            rows: &rows,
            output: &output,
            lookup_column: "tenant_name",
            registry_dir: &registry,
            require_full_resolution: true,
            target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
        })
        .expect_err("input row-count mismatch refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
        assert_eq!(refusal.detail["stage"], "apply");
        assert_eq!(refusal.detail["field"], "metadata.input.row_count");
        assert_eq!(refusal.detail["expected"], 2);
        assert_eq!(refusal.detail["actual"], 1);
        assert_eq!(refusal.detail["writes_performed"], false);
        assert_eq!(
            fs::read_to_string(&output).expect("output after refusal"),
            "sentinel output\n"
        );
        assert_no_apply_temp_sibling(temp.path());
    }

    fn assert_no_apply_temp_sibling(dir: &Path) {
        let leaked_temp = fs::read_dir(dir)
            .expect("tempdir entries")
            .map(|entry| entry.expect("entry").file_name())
            .any(|name| {
                name.to_str()
                    .is_some_and(|name| name.contains(".canon-apply."))
            });
        assert!(!leaked_temp, "apply temp output should not exist");
    }

    fn v1_apply_test_source_artifact(
        registry_hash: &str,
        input_hash: &str,
        row_count: u64,
        root_dir: &str,
    ) -> Value {
        let contract =
            crate::entity::schema::entity_v1_contract_for_stage(EntityArtifactStageV1::Run)
                .expect("run v1 contract");
        let mut artifact = json!({
            "version": CANON_ENTITY_RUN_VERSION_V1,
            "artifact_content_hash": "",
            "metadata": {
                "profile": {
                    "id": "cmbs_tenant_label",
                    "version": "1.0.0",
                    "entity_type": "organization",
                    "identity_semantics": "canonical_display_label",
                    "canonical_type": "tenant_label",
                    "patch_namespaces": {
                        "aliases": "cmbs_tenant_label.aliases",
                        "distinct": "cmbs_tenant_label.distinct",
                        "relations": "cmbs_tenant_label.relations"
                    },
                    "content_hash": "blake3:profile"
                },
                "strategy": {
                    "id": "apply-test-strategy",
                    "version": "1.0.0",
                    "content_hash": "blake3:strategy"
                },
                "registry_snapshot": {
                    "id": "cmbs-tenants",
                    "version": "1.0.0",
                    "source": "registry",
                    "lookup_snapshot_hash": registry_hash
                },
                "input": {
                    "row_count": row_count,
                    "content_hash": input_hash
                },
                "patch_namespace": "cmbs_tenant_label.aliases",
                "schema": {
                    "key": CANON_ENTITY_RUN_VERSION_V1,
                    "content_hash": crate::entity::schema::entity_v1_schema_content_hash(contract)
                        .expect("run schema hash")
                },
                "workdir": {
                    "root_dir": root_dir,
                    "stage_dir": contract.stage_dir,
                    "artifact_relpath": contract.artifact_relpath,
                    "payload_relpath": contract.payload_relpath
                },
                "upstream_artifacts": [],
                "artifact_content_hash": ""
            },
            "summary": {
                "counts": {
                    "rows": row_count
                },
                "labels": {
                    "stage": "run"
                }
            },
            "run_manifest_path": "run/manifest.json"
        });
        finalize_entity_v1_self_hash(&mut artifact).expect("source self hash");
        artifact
    }

    fn write_registry(path: &Path) {
        fs::create_dir_all(path).expect("registry dir");
        fs::write(
            path.join("registry.json"),
            r#"{
  "id": "cmbs-tenants",
  "version": "1.0.0",
  "description": "apply test registry",
  "updated": "2026-07-11",
  "entry_count": 1,
  "entity_profile": {
    "id": "cmbs_tenant_label",
    "identity_semantics": "canonical_display_label"
  }
}"#,
        )
        .expect("registry.json");
        fs::write(
            path.join("aliases.json"),
            r#"[
  {
    "input": "Sears",
    "canonical_id": "TNT-SEARS",
    "canonical_type": "tenant_label",
    "rule_id": "ENTITY_REVIEW_PROMOTE"
  }
]"#,
        )
        .expect("aliases.json");
    }
}
