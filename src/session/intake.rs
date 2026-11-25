//! Evidence Machine file intake for entity sessions.
//!
//! This module is deliberately a boundary adapter. It validates input digests
//! and handed artifacts/receipts, projects `warehouse_read` rows into the
//! existing session JSONL shapes, and delegates to the normal session writers
//! in `super`. Parquet rows are read incrementally into a temporary JSONL file.
//! Canon never acquires data or calls an external service here.

use super::{
    SessionAssertOutput, SessionAssertRequest, SessionIngestOutput, SessionIngestRequest,
    assert_evidence, ingest,
};
use crate::{Refusal, RefusalCode};
use parquet::file::reader::{FileReader, SerializedFileReader};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const ARTIFACT_VERSION: &str = "artifact.v0";
const RECEIPT_VERSION: &str = "receipt.v0";

/// Optional external provenance and row-projection controls for one intake.
#[derive(Debug, Clone, Default)]
pub struct IntakeOptions {
    pub expect_digest: Option<String>,
    pub artifact: Option<PathBuf>,
    pub receipt: Option<PathBuf>,
    pub rows_field: Option<String>,
    pub column_map: Option<PathBuf>,
}

impl IntakeOptions {
    pub fn is_active(&self) -> bool {
        self.expect_digest.is_some()
            || self.artifact.is_some()
            || self.receipt.is_some()
            || self.rows_field.is_some()
            || self.column_map.is_some()
    }
}

/// Whether a path names a Parquet input. Standard input remains JSONL because
/// a stream has no reliable filename extension to inspect.
pub fn is_parquet_path(path: &Path) -> bool {
    path != Path::new("-")
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("parquet"))
}

#[derive(Debug, Clone)]
struct VerifiedInput {
    bytes: Option<Vec<u8>>,
    parquet_path: Option<PathBuf>,
    input_digest: String,
    result_digest: Option<String>,
    artifact_id: Option<String>,
    sensitivity: Option<String>,
    receipt_digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ObservationColumnMap {
    obs_id: ColumnRef,
    entity_type: ColumnRef,
    surface: ColumnRef,
    #[serde(default)]
    keys: BTreeMap<String, ColumnRef>,
    #[serde(default)]
    attrs: BTreeMap<String, ColumnRef>,
    #[serde(default)]
    context: BTreeMap<String, ColumnRef>,
    source: ObservationSourceMap,
    #[serde(default)]
    origin_template: Option<String>,
    #[serde(default)]
    primary_key: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ObservationSourceMap {
    channel: ColumnRef,
    #[serde(default)]
    id: Option<ColumnRef>,
    #[serde(default)]
    lineage: Option<ColumnRef>,
    #[serde(default)]
    table: Option<ColumnRef>,
    #[serde(default)]
    file: Option<ColumnRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct EvidenceColumnMap {
    #[serde(rename = "type")]
    assertion_type: ColumnRef,
    left: ColumnRef,
    right: ColumnRef,
    asserted_by: ColumnRef,
    #[serde(default)]
    basis: BTreeMap<String, ColumnRef>,
    source: EvidenceSourceMap,
}

#[derive(Debug, Clone, Deserialize)]
struct EvidenceSourceMap {
    channel: ColumnRef,
    id: ColumnRef,
    #[serde(default)]
    locator: Option<ColumnRef>,
    #[serde(default)]
    content_hash: Option<ColumnRef>,
    #[serde(default)]
    lineage: Option<ColumnRef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ColumnRef {
    Column(String),
    Literal { literal: Value },
}

impl ColumnRef {
    fn name(&self) -> Option<&str> {
        match self {
            Self::Column(name) => Some(name),
            Self::Literal { .. } => None,
        }
    }
}

/// Verify, optionally project, and ingest an observation file.
pub fn ingest_with_options(
    session: &Path,
    observations: &Path,
    options: &IntakeOptions,
) -> Result<SessionIngestOutput, Refusal> {
    let verified = verify_input(observations, options)?;
    if let Some(parquet_path) = &verified.parquet_path {
        let prepared =
            write_parquet_observations(parquet_path, &verified, options.column_map.as_deref())?;
        let result = ingest(SessionIngestRequest {
            session: session.to_path_buf(),
            observations: prepared.path.clone(),
        });
        drop(prepared);
        return result;
    }
    let input_bytes = verified.bytes.as_deref().ok_or_else(|| {
        contract_refusal(
            observations,
            "input_format",
            "input bytes are unavailable for this file format",
            json!({"writes_performed": false}),
        )
    })?;
    let bytes = if let Some(column_map) = &options.column_map {
        project_observations(
            &verified,
            input_bytes,
            options.rows_field.as_deref(),
            column_map,
        )?
    } else {
        stamp_jsonl(input_bytes, &verified, None, false)?
    };
    let input = prepared_input_path(observations, &bytes, options.is_active())?;
    ingest(SessionIngestRequest {
        session: session.to_path_buf(),
        observations: input,
    })
}

/// Verify, optionally project, and assert an evidence file.
pub fn assert_with_options(
    session: &Path,
    evidence: &Path,
    options: &IntakeOptions,
) -> Result<SessionAssertOutput, Refusal> {
    let verified = verify_input(evidence, options)?;
    if let Some(parquet_path) = &verified.parquet_path {
        let prepared =
            write_parquet_evidence(parquet_path, &verified, options.column_map.as_deref())?;
        let result = assert_evidence(SessionAssertRequest {
            session: session.to_path_buf(),
            evidence: prepared.path.clone(),
        });
        drop(prepared);
        return result;
    }
    let input_bytes = verified.bytes.as_deref().ok_or_else(|| {
        contract_refusal(
            evidence,
            "input_format",
            "input bytes are unavailable for this file format",
            json!({"writes_performed": false}),
        )
    })?;
    let bytes = if let Some(column_map) = &options.column_map {
        project_evidence(
            &verified,
            input_bytes,
            options.rows_field.as_deref(),
            column_map,
        )?
    } else {
        stamp_jsonl(input_bytes, &verified, None, true)?
    };
    let input = prepared_input_path(evidence, &bytes, options.is_active())?;
    assert_evidence(SessionAssertRequest {
        session: session.to_path_buf(),
        evidence: input,
    })
}

fn verify_input(path: &Path, options: &IntakeOptions) -> Result<VerifiedInput, Refusal> {
    if options.receipt.is_some() && options.artifact.is_none() {
        return Err(contract_refusal(
            path,
            "receipt_requires_artifact",
            "--receipt requires --artifact so the receipt can name the artifact",
            json!({"field": "receipt", "writes_performed": false}),
        ));
    }
    if options.rows_field.is_some() != options.column_map.is_some() {
        return Err(contract_refusal(
            path,
            "column_map_pair",
            "--rows-field and --column-map must be supplied together",
            json!({"fields": ["rows_field", "column_map"], "writes_performed": false}),
        ));
    }

    let parquet_path = is_parquet_path(path).then(|| path.to_path_buf());
    let (bytes, actual_digest) = if parquet_path.is_some() {
        (None, hash_file(path)?)
    } else {
        let bytes = read_input(path)?;
        let digest = hash_bytes(&bytes);
        (Some(bytes), digest)
    };
    if let Some(expected) = &options.expect_digest
        && expected != &actual_digest
    {
        return Err(contract_refusal(
            path,
            "digest_mismatch",
            format!(
                "Input digest mismatch for '{}': expected {}, actual {}",
                path.display(),
                expected,
                actual_digest
            ),
            json!({
                "path": path.display().to_string(),
                "expected": expected,
                "actual": actual_digest,
                "writes_performed": false
            }),
        ));
    }

    let artifact = if let Some(artifact_path) = &options.artifact {
        let artifact_bytes =
            fs::read(artifact_path).map_err(|error| io_refusal(artifact_path, error))?;
        let artifact_value = parse_document(artifact_path, &artifact_bytes, "artifact")?;
        validate_artifact(&artifact_value, artifact_path, &actual_digest)?;
        Some((artifact_value, artifact_bytes))
    } else {
        None
    };

    let (artifact_value, artifact_id, sensitivity) = artifact
        .as_ref()
        .map(|(value, _)| {
            (
                value.clone(),
                value["artifact_id"].as_str().map(ToOwned::to_owned),
                value["sensitivity"].as_str().map(ToOwned::to_owned),
            )
        })
        .map_or((None, None, None), |(value, id, sensitivity)| {
            (Some(value), id, sensitivity)
        });

    let receipt_digest = if let Some(receipt_path) = &options.receipt {
        let receipt_bytes =
            fs::read(receipt_path).map_err(|error| io_refusal(receipt_path, error))?;
        let receipt_value = parse_document(receipt_path, &receipt_bytes, "receipt")?;
        validate_receipt(&receipt_value, artifact_id.as_deref(), receipt_path)?;
        Some(hash_bytes(&receipt_bytes))
    } else {
        None
    };

    Ok(VerifiedInput {
        parquet_path,
        bytes,
        input_digest: actual_digest.clone(),
        result_digest: if options.expect_digest.is_some() || artifact_value.is_some() {
            Some(actual_digest)
        } else {
            None
        },
        artifact_id,
        sensitivity,
        receipt_digest,
    })
}

fn validate_artifact(value: &Value, input: &Path, actual_digest: &str) -> Result<(), Refusal> {
    let object = value.as_object().ok_or_else(|| {
        contract_refusal(
            input,
            "artifact_not_object",
            "artifact.v0 must be a JSON object",
            json!({"field": "artifact", "writes_performed": false}),
        )
    })?;
    let fields = [
        "artifact_id",
        "as_of",
        "byte_hash",
        "derived_from",
        "kind",
        "media_type",
        "observed_at",
        "payload_ref",
        "schema",
        "scope_ref",
        "sensitivity",
    ];
    validate_required_fields(object, &fields, input, "artifact")?;
    reject_unknown_fields(object, &fields, input, "artifact")?;
    validate_ref(
        required_field(object, "artifact_id", input, "artifact")?,
        "artifact:blake3:",
        "artifact_id",
        input,
        "artifact",
    )?;
    validate_text_field(object, "as_of", input, "artifact")?;
    let byte_hash = required_field(object, "byte_hash", input, "artifact")?;
    validate_ref(byte_hash, "blake3:", "byte_hash", input, "artifact")?;
    if byte_hash != actual_digest {
        return Err(contract_refusal(
            input,
            "artifact_byte_hash",
            format!(
                "artifact byte_hash does not match input: expected {}, actual {}",
                display_value(byte_hash),
                actual_digest
            ),
            json!({
                "field": "byte_hash",
                "expected": byte_hash,
                "actual": actual_digest,
                "writes_performed": false
            }),
        ));
    }
    let derived_from = required_field(object, "derived_from", input, "artifact")?;
    let Some(derived_from) = derived_from.as_array() else {
        return Err(contract_refusal(
            input,
            "artifact_field_type",
            "artifact derived_from must be an array",
            json!({"field": "derived_from", "writes_performed": false}),
        ));
    };
    for (index, item) in derived_from.iter().enumerate() {
        validate_ref(
            item,
            "artifact:blake3:",
            &format!("derived_from[{index}]"),
            input,
            "artifact",
        )?;
    }
    validate_nonempty_text_field(object, "kind", input, "artifact")?;
    validate_text_field(object, "media_type", input, "artifact")?;
    validate_text_field(object, "observed_at", input, "artifact")?;
    validate_ref(
        required_field(object, "payload_ref", input, "artifact")?,
        "cas:blake3:",
        "payload_ref",
        input,
        "artifact",
    )?;
    if required_field(object, "schema", input, "artifact")? != ARTIFACT_VERSION {
        return Err(contract_refusal(
            input,
            "artifact_schema",
            format!(
                "artifact schema must be {}, got {}",
                ARTIFACT_VERSION,
                display_value(required_field(object, "schema", input, "artifact")?)
            ),
            json!({
                "field": "schema",
                "expected": ARTIFACT_VERSION,
                "actual": object["schema"],
                "writes_performed": false
            }),
        ));
    }
    validate_ref(
        required_field(object, "scope_ref", input, "artifact")?,
        "blake3:",
        "scope_ref",
        input,
        "artifact",
    )?;
    if !matches!(
        required_field(object, "sensitivity", input, "artifact")?.as_str(),
        Some("public" | "test_restricted")
    ) {
        return Err(contract_refusal(
            input,
            "artifact_sensitivity",
            "artifact sensitivity must be public or test_restricted",
            json!({"field": "sensitivity", "writes_performed": false}),
        ));
    }
    Ok(())
}

fn validate_receipt(value: &Value, artifact_id: Option<&str>, path: &Path) -> Result<(), Refusal> {
    let object = value.as_object().ok_or_else(|| {
        contract_refusal(
            path,
            "receipt_not_object",
            "receipt.v0 must be a JSON object",
            json!({"field": "receipt", "writes_performed": false}),
        )
    })?;
    let fields = [
        "artifacts",
        "claim_state",
        "code",
        "context",
        "coverage",
        "domain_disposition",
        "execution",
        "message",
        "observed_at",
        "schema",
    ];
    validate_required_fields(object, &fields, path, "receipt")?;
    reject_unknown_fields(object, &fields, path, "receipt")?;
    if required_field(object, "schema", path, "receipt")? != RECEIPT_VERSION {
        return Err(contract_refusal(
            path,
            "receipt_schema",
            format!(
                "receipt schema must be {}, got {}",
                RECEIPT_VERSION,
                display_value(&object["schema"])
            ),
            json!({
                "field": "schema",
                "expected": RECEIPT_VERSION,
                "actual": required_field(object, "schema", path, "receipt")?,
                "writes_performed": false
            }),
        ));
    }
    let artifacts = required_field(object, "artifacts", path, "receipt")?
        .as_array()
        .ok_or_else(|| {
            contract_refusal(
                path,
                "receipt_field_type",
                "receipt artifacts must be an array",
                json!({"field": "artifacts", "writes_performed": false}),
            )
        })?;
    for (index, item) in artifacts.iter().enumerate() {
        validate_ref(
            item,
            "artifact:blake3:",
            &format!("artifacts[{index}]"),
            path,
            "receipt",
        )?;
    }
    validate_enum_field(
        object,
        "claim_state",
        &[
            "conflicted",
            "contradicted",
            "forced",
            "supported",
            "unresolved",
        ],
        path,
        "receipt",
    )?;
    if !matches!(
        required_field(object, "code", path, "receipt")?,
        Value::Null | Value::String(_)
    ) {
        return Err(contract_refusal(
            path,
            "receipt_field_type",
            "receipt code must be a string or null",
            json!({"field": "code", "writes_performed": false}),
        ));
    }
    validate_receipt_context(required_field(object, "context", path, "receipt")?, path)?;
    validate_enum_field(
        object,
        "coverage",
        &["complete", "partial", "unattempted"],
        path,
        "receipt",
    )?;
    validate_enum_field(
        object,
        "domain_disposition",
        &["negative", "not_applicable", "positive", "refusal"],
        path,
        "receipt",
    )?;
    validate_enum_field(
        object,
        "execution",
        &["cancelled", "err", "ok", "panicked"],
        path,
        "receipt",
    )?;
    validate_text_field(object, "message", path, "receipt")?;
    validate_text_field(object, "observed_at", path, "receipt")?;
    for (field, expected) in [
        ("execution", "ok"),
        ("domain_disposition", "positive"),
        ("coverage", "complete"),
    ] {
        if object[field] != expected {
            return Err(contract_refusal(
                path,
                "receipt_state",
                format!(
                    "receipt {field} must be {expected}, got {}",
                    display_value(&object[field])
                ),
                json!({
                    "field": field,
                    "expected": expected,
                    "actual": object[field],
                    "writes_performed": false
                }),
            ));
        }
    }
    let Some(artifact_id) = artifact_id else {
        return Err(contract_refusal(
            path,
            "receipt_artifact",
            "receipt validation requires a verified artifact id",
            json!({"field": "artifacts", "writes_performed": false}),
        ));
    };
    let listed = artifacts
        .iter()
        .any(|item| item.as_str() == Some(artifact_id));
    if !listed {
        return Err(contract_refusal(
            path,
            "receipt_artifact",
            format!("receipt artifacts does not list artifact_id {artifact_id}"),
            json!({
                "field": "artifacts",
                "expected": artifact_id,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_required_fields(
    object: &Map<String, Value>,
    fields: &[&str],
    path: &Path,
    label: &str,
) -> Result<(), Refusal> {
    for field in fields {
        if !object.contains_key(*field) {
            return Err(contract_refusal(
                path,
                &format!("{label}_missing_field"),
                format!("{label} is missing required field '{field}'"),
                json!({"field": field, "writes_performed": false}),
            ));
        }
    }
    Ok(())
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    fields: &[&str],
    path: &Path,
    label: &str,
) -> Result<(), Refusal> {
    if let Some(field) = object
        .keys()
        .find(|field| !fields.contains(&field.as_str()))
    {
        return Err(contract_refusal(
            path,
            &format!("{label}_unknown_field"),
            format!("{label} contains unknown field '{field}'"),
            json!({"field": field, "writes_performed": false}),
        ));
    }
    Ok(())
}

fn required_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    path: &Path,
    label: &str,
) -> Result<&'a Value, Refusal> {
    object.get(field).ok_or_else(|| {
        contract_refusal(
            path,
            &format!("{label}_missing_field"),
            format!("{label} is missing required field '{field}'"),
            json!({"field": field, "writes_performed": false}),
        )
    })
}

fn validate_text_field(
    object: &Map<String, Value>,
    field: &str,
    path: &Path,
    label: &str,
) -> Result<(), Refusal> {
    if !required_field(object, field, path, label)?.is_string() {
        return Err(contract_refusal(
            path,
            &format!("{label}_field_type"),
            format!("{label} {field} must be a string"),
            json!({"field": field, "writes_performed": false}),
        ));
    }
    Ok(())
}

fn validate_nonempty_text_field(
    object: &Map<String, Value>,
    field: &str,
    path: &Path,
    label: &str,
) -> Result<(), Refusal> {
    let valid = required_field(object, field, path, label)?
        .as_str()
        .is_some_and(|value| !value.is_empty());
    if !valid {
        return Err(contract_refusal(
            path,
            &format!("{label}_field_type"),
            format!("{label} {field} must be a non-empty string"),
            json!({"field": field, "writes_performed": false}),
        ));
    }
    Ok(())
}

fn validate_ref(
    value: &Value,
    prefix: &str,
    field: &str,
    path: &Path,
    label: &str,
) -> Result<(), Refusal> {
    let valid = value
        .as_str()
        .and_then(|value| value.strip_prefix(prefix))
        .is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        });
    if !valid {
        return Err(contract_refusal(
            path,
            &format!("{label}_field_format"),
            format!("{label} {field} must match {prefix}<64 lowercase hex digits>"),
            json!({"field": field, "writes_performed": false}),
        ));
    }
    Ok(())
}

fn validate_enum_field(
    object: &Map<String, Value>,
    field: &str,
    allowed: &[&str],
    path: &Path,
    label: &str,
) -> Result<(), Refusal> {
    let valid = required_field(object, field, path, label)?
        .as_str()
        .is_some_and(|value| allowed.contains(&value));
    if !valid {
        return Err(contract_refusal(
            path,
            &format!("{label}_field_value"),
            format!("{label} {field} is not one of the permitted values"),
            json!({"field": field, "allowed": allowed, "writes_performed": false}),
        ));
    }
    Ok(())
}

fn validate_receipt_context(value: &Value, path: &Path) -> Result<(), Refusal> {
    let Some(object) = value.as_object() else {
        return Err(contract_refusal(
            path,
            "receipt_context",
            "receipt context must be an object",
            json!({"field": "context", "writes_performed": false}),
        ));
    };
    match object.get("kind").and_then(Value::as_str) {
        Some("standalone") if object.len() == 1 => Ok(()),
        Some("node")
            if object.len() == 3
                && object
                    .get("node_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty())
                && object
                    .get("run_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty()) =>
        {
            Ok(())
        }
        _ => Err(contract_refusal(
            path,
            "receipt_context",
            "receipt context must be standalone or a node with non-empty node_id and run_id",
            json!({"field": "context", "writes_performed": false}),
        )),
    }
}

fn project_observations(
    verified: &VerifiedInput,
    input_bytes: &[u8],
    rows_field: Option<&str>,
    map_path: &Path,
) -> Result<Vec<u8>, Refusal> {
    let rows_field = rows_field.ok_or_else(|| {
        contract_refusal(
            map_path,
            "rows_field_missing",
            "a rows field is required with --column-map",
            json!({"field": "rows_field", "writes_performed": false}),
        )
    })?;
    let map = parse_yaml::<ObservationColumnMap>(map_path)?;
    let payload = parse_document(map_path, input_bytes, "rows payload")?;
    let rows = payload[rows_field].as_array().ok_or_else(|| {
        contract_refusal(
            map_path,
            "rows_field",
            format!("rows payload field '{rows_field}' must be an array"),
            json!({"field": rows_field, "writes_performed": false}),
        )
    })?;
    validate_observation_columns(rows, &map, map_path)?;
    let catalog_key = payload.get("catalog_key").cloned();
    let mut lines = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let observation = project_observation_row(
            row,
            &map,
            verified,
            catalog_key.as_ref(),
            index + 1,
            map_path,
        )?;
        lines.push(serde_json::to_string(&observation).map_err(|error| {
            contract_refusal(
                map_path,
                "row_projection",
                format!("failed to serialize mapped row {}: {error}", index + 1),
                json!({"row": index + 1, "writes_performed": false}),
            )
        })?);
    }
    Ok(lines.join("\n").into_bytes())
}

fn project_observation_row(
    value: &Value,
    map: &ObservationColumnMap,
    verified: &VerifiedInput,
    catalog_key: Option<&Value>,
    row_number: usize,
    map_path: &Path,
) -> Result<Value, Refusal> {
    let row = value.as_object().ok_or_else(|| {
        contract_refusal(
            map_path,
            "row_not_object",
            format!("row {row_number} must be an object"),
            json!({"row": row_number, "writes_performed": false}),
        )
    })?;
    let mut observation = Map::new();
    observation.insert(
        "obs_id".to_string(),
        mapped_value(row, &map.obs_id, "obs_id")?,
    );
    observation.insert(
        "entity_type".to_string(),
        mapped_value(row, &map.entity_type, "entity_type")?,
    );
    observation.insert(
        "surface".to_string(),
        mapped_value(row, &map.surface, "surface")?,
    );
    observation.insert("keys".to_string(), mapped_object(row, &map.keys, "keys")?);
    observation.insert(
        "attrs".to_string(),
        mapped_object(row, &map.attrs, "attrs")?,
    );
    observation.insert(
        "context".to_string(),
        mapped_object(row, &map.context, "context")?,
    );

    let mut source = row
        .get("source")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    source.insert(
        "channel".to_string(),
        mapped_value(row, &map.source.channel, "source.channel")?,
    );
    insert_optional_mapped(row, &mut source, "id", map.source.id.as_ref(), "source.id")?;
    insert_optional_mapped(
        row,
        &mut source,
        "lineage",
        map.source.lineage.as_ref(),
        "source.lineage",
    )?;
    insert_optional_mapped(
        row,
        &mut source,
        "table",
        map.source.table.as_ref(),
        "source.table",
    )?;
    insert_optional_mapped(
        row,
        &mut source,
        "file",
        map.source.file.as_ref(),
        "source.file",
    )?;
    if let Some(template) = &map.origin_template {
        source.insert(
            "lineage".to_string(),
            Value::String(expand_template(template, row, map_path)?),
        );
    }
    if let Some(catalog_key) = catalog_key {
        source.insert("catalog_key".to_string(), catalog_key.clone());
    }
    let mut columns = Map::new();
    columns.insert("surface".to_string(), ref_name(&map.surface));
    columns.insert("keys".to_string(), mapped_names(&map.keys));
    source.insert("columns".to_string(), Value::Object(columns));
    stamp_source(&mut source, verified, None, &map.primary_key, row);
    observation.insert("source".to_string(), Value::Object(source));
    Ok(Value::Object(observation))
}

fn project_evidence(
    verified: &VerifiedInput,
    input_bytes: &[u8],
    rows_field: Option<&str>,
    map_path: &Path,
) -> Result<Vec<u8>, Refusal> {
    let rows_field = rows_field.ok_or_else(|| {
        contract_refusal(
            map_path,
            "rows_field_missing",
            "a rows field is required with --column-map",
            json!({"field": "rows_field", "writes_performed": false}),
        )
    })?;
    let map = parse_yaml::<EvidenceColumnMap>(map_path)?;
    let payload = parse_document(map_path, input_bytes, "rows payload")?;
    let rows = payload[rows_field].as_array().ok_or_else(|| {
        contract_refusal(
            map_path,
            "rows_field",
            format!("rows payload field '{rows_field}' must be an array"),
            json!({"field": rows_field, "writes_performed": false}),
        )
    })?;
    validate_evidence_columns(rows, &map, map_path)?;
    let mut lines = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let assertion = project_evidence_row(row, &map, verified, index + 1, map_path)?;
        lines.push(serde_json::to_string(&assertion).map_err(|error| {
            contract_refusal(
                map_path,
                "row_projection",
                format!(
                    "failed to serialize mapped evidence row {}: {error}",
                    index + 1
                ),
                json!({"row": index + 1, "writes_performed": false}),
            )
        })?);
    }
    Ok(lines.join("\n").into_bytes())
}

fn project_evidence_row(
    value: &Value,
    map: &EvidenceColumnMap,
    verified: &VerifiedInput,
    row_number: usize,
    map_path: &Path,
) -> Result<Value, Refusal> {
    let row = value.as_object().ok_or_else(|| {
        contract_refusal(
            map_path,
            "row_not_object",
            format!("row {row_number} must be an object"),
            json!({"row": row_number, "writes_performed": false}),
        )
    })?;
    let mut assertion = Map::new();
    assertion.insert(
        "type".to_string(),
        mapped_value(row, &map.assertion_type, "type")?,
    );
    assertion.insert("left".to_string(), mapped_value(row, &map.left, "left")?);
    assertion.insert("right".to_string(), mapped_value(row, &map.right, "right")?);
    assertion.insert(
        "asserted_by".to_string(),
        mapped_value(row, &map.asserted_by, "asserted_by")?,
    );
    assertion.insert(
        "basis".to_string(),
        mapped_object(row, &map.basis, "basis")?,
    );
    let mut source = Map::new();
    source.insert(
        "channel".to_string(),
        mapped_value(row, &map.source.channel, "source.channel")?,
    );
    source.insert(
        "id".to_string(),
        mapped_value(row, &map.source.id, "source.id")?,
    );
    insert_optional_mapped(
        row,
        &mut source,
        "locator",
        map.source.locator.as_ref(),
        "source.locator",
    )?;
    insert_optional_mapped(
        row,
        &mut source,
        "content_hash",
        map.source.content_hash.as_ref(),
        "source.content_hash",
    )?;
    insert_optional_mapped(
        row,
        &mut source,
        "lineage",
        map.source.lineage.as_ref(),
        "source.lineage",
    )?;
    if let Some(result_digest) = &verified.result_digest
        && !source.contains_key("content_hash")
    {
        source.insert(
            "content_hash".to_string(),
            Value::String(result_digest.clone()),
        );
    }
    assertion.insert("source".to_string(), Value::Object(source));
    Ok(Value::Object(assertion))
}

fn write_parquet_observations(
    parquet_path: &Path,
    verified: &VerifiedInput,
    column_map_path: Option<&Path>,
) -> Result<PreparedInputFile, Refusal> {
    let map = column_map_path
        .map(parse_yaml::<ObservationColumnMap>)
        .transpose()?;
    let (prepared, file) = PreparedInputFile::create()?;
    let mut output = BufWriter::new(file);
    visit_parquet_rows(parquet_path, |row_number, row| {
        let observation = if let Some(map) = &map {
            let map_path = column_map_path.unwrap_or(parquet_path);
            validate_observation_columns(std::slice::from_ref(&row), map, map_path)?;
            let catalog_key = row.get("catalog_key");
            project_observation_row(&row, map, verified, catalog_key, row_number, map_path)?
        } else {
            stamp_direct_record(row, verified, false)
        };
        write_json_line(&mut output, &observation, parquet_path, row_number)
    })?;
    output
        .flush()
        .map_err(|error| io_refusal(&prepared.path, error))?;
    verify_input_unchanged(parquet_path, verified)?;
    Ok(prepared)
}

fn write_parquet_evidence(
    parquet_path: &Path,
    verified: &VerifiedInput,
    column_map_path: Option<&Path>,
) -> Result<PreparedInputFile, Refusal> {
    let map = column_map_path
        .map(parse_yaml::<EvidenceColumnMap>)
        .transpose()?;
    let (prepared, file) = PreparedInputFile::create()?;
    let mut output = BufWriter::new(file);
    visit_parquet_rows(parquet_path, |row_number, row| {
        let evidence = if let Some(map) = &map {
            let map_path = column_map_path.unwrap_or(parquet_path);
            validate_evidence_columns(std::slice::from_ref(&row), map, map_path)?;
            project_evidence_row(&row, map, verified, row_number, map_path)?
        } else {
            stamp_direct_record(row, verified, true)
        };
        write_json_line(&mut output, &evidence, parquet_path, row_number)
    })?;
    output
        .flush()
        .map_err(|error| io_refusal(&prepared.path, error))?;
    verify_input_unchanged(parquet_path, verified)?;
    Ok(prepared)
}

fn visit_parquet_rows(
    path: &Path,
    mut visit: impl FnMut(usize, Value) -> Result<(), Refusal>,
) -> Result<(), Refusal> {
    let file = File::open(path).map_err(|error| io_refusal(path, error))?;
    let reader = SerializedFileReader::new(file).map_err(|error| {
        contract_refusal(
            path,
            "parquet_read",
            format!("unable to open Parquet input '{}': {error}", path.display()),
            json!({"writes_performed": false}),
        )
    })?;
    let rows = reader.get_row_iter(None).map_err(|error| {
        contract_refusal(
            path,
            "parquet_read",
            format!(
                "unable to read Parquet rows from '{}': {error}",
                path.display()
            ),
            json!({"writes_performed": false}),
        )
    })?;
    for (index, row) in rows.enumerate() {
        let row = row.map_err(|error| {
            contract_refusal(
                path,
                "parquet_read",
                format!("unable to read Parquet row {}: {error}", index + 1),
                json!({"row": index + 1, "writes_performed": false}),
            )
        })?;
        visit(index + 1, row.to_json_value())?;
    }
    Ok(())
}

fn stamp_direct_record(mut value: Value, verified: &VerifiedInput, assertion: bool) -> Value {
    if let Some(object) = value.as_object_mut()
        && let Some(source) = object.get_mut("source").and_then(Value::as_object_mut)
    {
        stamp_source(source, verified, None, &[], &Map::new());
        if assertion
            && let Some(result_digest) = &verified.result_digest
            && !source.contains_key("content_hash")
        {
            source.insert(
                "content_hash".to_string(),
                Value::String(result_digest.clone()),
            );
        }
    }
    value
}

fn write_json_line(
    output: &mut impl Write,
    value: &Value,
    input_path: &Path,
    row_number: usize,
) -> Result<(), Refusal> {
    serde_json::to_writer(&mut *output, value).map_err(|error| {
        contract_refusal(
            input_path,
            "row_projection",
            format!("failed to serialize Parquet row {row_number}: {error}"),
            json!({"row": row_number, "writes_performed": false}),
        )
    })?;
    output
        .write_all(b"\n")
        .map_err(|error| io_refusal(input_path, error))
}

fn verify_input_unchanged(path: &Path, verified: &VerifiedInput) -> Result<(), Refusal> {
    if let Some(expected) = verified
        .parquet_path
        .as_ref()
        .map(|_| &verified.input_digest)
    {
        let actual = hash_file(path)?;
        if &actual != expected {
            return Err(contract_refusal(
                path,
                "input_changed_during_read",
                "Parquet input changed while it was being read",
                json!({
                    "expected": expected,
                    "actual": actual,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

struct PreparedInputFile {
    path: PathBuf,
}

impl PreparedInputFile {
    fn create() -> Result<(Self, File), Refusal> {
        static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let sequence = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "canon-session-intake-{}-{sequence}.jsonl",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok((Self { path }, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(io_refusal(&path, error)),
            }
        }
        Err(contract_refusal(
            &std::env::temp_dir(),
            "temporary_input_unavailable",
            "unable to allocate a unique temporary session input file",
            json!({"writes_performed": false}),
        ))
    }
}

impl Drop for PreparedInputFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn validate_observation_columns(
    rows: &[Value],
    map: &ObservationColumnMap,
    path: &Path,
) -> Result<(), Refusal> {
    let mut refs = vec![
        ("obs_id", &map.obs_id),
        ("entity_type", &map.entity_type),
        ("surface", &map.surface),
        ("source.channel", &map.source.channel),
    ];
    if let Some(reference) = &map.source.id {
        refs.push(("source.id", reference));
    }
    if let Some(reference) = &map.source.lineage {
        refs.push(("source.lineage", reference));
    }
    if let Some(reference) = &map.source.table {
        refs.push(("source.table", reference));
    }
    if let Some(reference) = &map.source.file {
        refs.push(("source.file", reference));
    }
    for (name, reference) in &map.keys {
        refs.push((name.as_str(), reference));
    }
    for (name, reference) in &map.attrs {
        refs.push((name.as_str(), reference));
    }
    for (name, reference) in &map.context {
        refs.push((name.as_str(), reference));
    }
    for (field, reference) in refs {
        ensure_column_present(rows, reference, field, path)?;
    }
    Ok(())
}

fn validate_evidence_columns(
    rows: &[Value],
    map: &EvidenceColumnMap,
    path: &Path,
) -> Result<(), Refusal> {
    for (field, reference) in [
        ("type", &map.assertion_type),
        ("left", &map.left),
        ("right", &map.right),
        ("asserted_by", &map.asserted_by),
        ("source.channel", &map.source.channel),
        ("source.id", &map.source.id),
    ] {
        ensure_column_present(rows, reference, field, path)?;
    }
    for (name, reference) in &map.basis {
        ensure_column_present(rows, reference, name, path)?;
    }
    Ok(())
}

fn ensure_column_present(
    rows: &[Value],
    reference: &ColumnRef,
    field: &str,
    path: &Path,
) -> Result<(), Refusal> {
    if let Some(name) = reference.name()
        && !rows
            .iter()
            .any(|row| row.as_object().is_some_and(|row| row.contains_key(name)))
    {
        return Err(contract_refusal(
            path,
            "column_missing",
            format!("column map field '{field}' names missing source column '{name}'"),
            json!({
                "field": field,
                "column": name,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn mapped_value(
    row: &Map<String, Value>,
    reference: &ColumnRef,
    _field: &str,
) -> Result<Value, Refusal> {
    match reference {
        ColumnRef::Column(name) => Ok(row.get(name).cloned().unwrap_or(Value::Null)),
        ColumnRef::Literal { literal } => Ok(literal.clone()),
    }
}

fn mapped_object(
    row: &Map<String, Value>,
    references: &BTreeMap<String, ColumnRef>,
    field: &str,
) -> Result<Value, Refusal> {
    let mut object = Map::new();
    for (name, reference) in references {
        object.insert(name.clone(), mapped_value(row, reference, field)?);
    }
    Ok(Value::Object(object))
}

fn insert_optional_mapped(
    row: &Map<String, Value>,
    target: &mut Map<String, Value>,
    name: &str,
    reference: Option<&ColumnRef>,
    field: &str,
) -> Result<(), Refusal> {
    if let Some(reference) = reference {
        target.insert(name.to_string(), mapped_value(row, reference, field)?);
    }
    Ok(())
}

fn mapped_names(references: &BTreeMap<String, ColumnRef>) -> Value {
    Value::Object(
        references
            .iter()
            .map(|(name, reference)| (name.clone(), ref_name(reference)))
            .collect(),
    )
}

fn ref_name(reference: &ColumnRef) -> Value {
    reference
        .name()
        .map(|name| Value::String(name.to_string()))
        .unwrap_or(Value::Null)
}

fn expand_template(
    template: &str,
    row: &Map<String, Value>,
    path: &Path,
) -> Result<String, Refusal> {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('}').ok_or_else(|| {
            contract_refusal(
                path,
                "origin_template",
                "origin_template contains an unmatched '{'",
                json!({"field": "origin_template", "writes_performed": false}),
            )
        })?;
        let name = &after[..end];
        let value = row.get(name).and_then(Value::as_str).ok_or_else(|| {
            contract_refusal(
                path,
                "origin_template",
                format!("origin_template column '{name}' is absent or non-string"),
                json!({"field": "origin_template", "column": name, "writes_performed": false}),
            )
        })?;
        output.push_str(value);
        rest = &after[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

fn stamp_jsonl(
    bytes: &[u8],
    verified: &VerifiedInput,
    columns: Option<&Value>,
    assertion: bool,
) -> Result<Vec<u8>, Refusal> {
    let mut lines = Vec::new();
    for line in String::from_utf8_lossy(bytes).lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(mut value) = serde_json::from_str::<Value>(line) else {
            lines.push(line.to_string());
            continue;
        };
        if let Some(object) = value.as_object_mut()
            && let Some(source) = object.get_mut("source").and_then(Value::as_object_mut)
        {
            stamp_source(source, verified, columns, &[], &Map::new());
            if assertion
                && let Some(result_digest) = &verified.result_digest
                && !source.contains_key("content_hash")
            {
                source.insert(
                    "content_hash".to_string(),
                    Value::String(result_digest.clone()),
                );
            }
        }
        lines.push(serde_json::to_string(&value).map_err(|error| {
            contract_refusal(
                Path::new("<input>"),
                "input_projection",
                format!("failed to stamp JSONL input: {error}"),
                json!({"writes_performed": false}),
            )
        })?);
    }
    Ok(lines.join("\n").into_bytes())
}

fn stamp_source(
    source: &mut Map<String, Value>,
    verified: &VerifiedInput,
    columns: Option<&Value>,
    primary_key: &[String],
    row: &Map<String, Value>,
) {
    if let Some(result_digest) = &verified.result_digest {
        source.insert(
            "result_digest".to_string(),
            Value::String(result_digest.clone()),
        );
    }
    if let Some(artifact_id) = &verified.artifact_id {
        source.insert(
            "artifact_id".to_string(),
            Value::String(artifact_id.clone()),
        );
    }
    if let Some(receipt_digest) = &verified.receipt_digest {
        source.insert(
            "receipt_digest".to_string(),
            Value::String(receipt_digest.clone()),
        );
    }
    if let Some(sensitivity) = &verified.sensitivity {
        source.insert(
            "sensitivity".to_string(),
            Value::String(sensitivity.clone()),
        );
    }
    if let Some(columns) = columns {
        source.insert("columns".to_string(), columns.clone());
    }
    if !source.contains_key("row_hash")
        && !source.contains_key("row_key")
        && let Some(row_hash) = row.get("row_hash")
    {
        source.insert("row_hash".to_string(), row_hash.clone());
        return;
    }
    if !source.contains_key("row_hash")
        && !source.contains_key("row_key")
        && !primary_key.is_empty()
    {
        source.insert(
            "row_key".to_string(),
            Value::Array(
                primary_key
                    .iter()
                    .map(|key| row.get(key).cloned().unwrap_or(Value::Null))
                    .collect(),
            ),
        );
    }
}

fn prepared_input_path(original: &Path, bytes: &[u8], active: bool) -> Result<PathBuf, Refusal> {
    if !active && original != Path::new("-") {
        return Ok(original.to_path_buf());
    }
    let digest = hash_bytes(bytes);
    let directory = std::env::temp_dir().join(format!(
        "canon-session-intake-{}-{}",
        std::process::id(),
        digest.trim_start_matches("blake3:")
    ));
    fs::create_dir_all(&directory).map_err(|error| io_refusal(&directory, error))?;
    let path = directory.join("prepared.jsonl");
    fs::write(&path, bytes).map_err(|error| io_refusal(&path, error))?;
    Ok(path)
}

fn parse_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Refusal> {
    let bytes = fs::read(path).map_err(|error| io_refusal(path, error))?;
    let text = String::from_utf8(bytes).map_err(|error| {
        contract_refusal(
            path,
            "column_map_encoding",
            format!("column map is not UTF-8: {error}"),
            json!({"path": path.display().to_string(), "writes_performed": false}),
        )
    })?;
    serde_yaml::from_str(&text).map_err(|error| {
        contract_refusal(
            path,
            "column_map_parse",
            format!("column map is not valid YAML: {error}"),
            json!({"path": path.display().to_string(), "writes_performed": false}),
        )
    })
}

fn parse_document(path: &Path, bytes: &[u8], label: &str) -> Result<Value, Refusal> {
    serde_json::from_slice(bytes).map_err(|error| {
        contract_refusal(
            path,
            "document_parse",
            format!("{label} '{}' is not valid JSON: {error}", path.display()),
            json!({"path": path.display().to_string(), "writes_performed": false}),
        )
    })
}

fn read_input(path: &Path) -> Result<Vec<u8>, Refusal> {
    if path == Path::new("-") {
        let mut bytes = Vec::new();
        io::stdin()
            .lock()
            .read_to_end(&mut bytes)
            .map_err(|error| io_refusal(path, error))?;
        Ok(bytes)
    } else {
        fs::read(path).map_err(|error| io_refusal(path, error))
    }
}

fn hash_file(path: &Path) -> Result<String, Refusal> {
    let mut file = File::open(path).map_err(|error| io_refusal(path, error))?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut hasher = blake3::Hasher::new();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| io_refusal(path, error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn display_value(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn contract_refusal(path: &Path, code: &str, message: impl Into<String>, detail: Value) -> Refusal {
    let mut detail = detail;
    if let Some(object) = detail.as_object_mut() {
        object.insert(
            "path".to_string(),
            Value::String(path.display().to_string()),
        );
        object.insert("code".to_string(), Value::String(code.to_string()));
        object
            .entry("writes_performed".to_string())
            .or_insert(Value::Bool(false));
    }
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.into(),
        detail,
        next_command: Some("Repair the handed artifact, receipt, digest, or column map, then rerun the same session command".to_string()),
    }
}

fn io_refusal(path: &Path, error: io::Error) -> Refusal {
    Refusal {
        code: RefusalCode::EIo,
        message: format!("Unable to access '{}': {error}", path.display()),
        detail: json!({"path": path.display().to_string(), "error": error.to_string(), "writes_performed": false}),
        next_command: Some(
            "Check the handed file path and rerun the same session command".to_string(),
        ),
    }
}
