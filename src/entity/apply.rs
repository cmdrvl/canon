//! Apply-stage streaming exact replay helpers.
//!
//! This module owns the bounded row adapter only. Promotion, audit validation,
//! and registry mutation safety are separate ENT-P09 surfaces; apply receives a
//! deterministic replay table and appends canonical fields without rewriting
//! the raw input row bytes.

use crate::entity::{
    CANON_ENTITY_APPLY_VERSION,
    error::EntityRefusalKind,
    stream::{
        EntityStreamChunkMetadata, EntityStreamFormat, EntityStreamInput,
        EntityStreamRowProvenance, EntityStreamStage, EntityStreamTelemetry,
        deterministic_chunk_metadata, stream_telemetry,
    },
};
use crate::{Refusal, witness};
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_APPLY_ROWS_PER_CHUNK: u64 = 1024;
pub const APPLY_CANONICAL_FIELDS: &[&str] =
    &["canonical_id", "canonical_type", "canonical_rule_id"];

const MAX_APPLY_PROVENANCE_SAMPLES: usize = 16;

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

impl ApplyCanonicalResolution {
    fn fields(&self) -> [&str; 3] {
        [&self.canonical_id, &self.canonical_type, &self.rule_id]
    }
}

#[derive(Debug, Clone)]
pub struct ApplyStreamRequest<'a> {
    pub rows: &'a Path,
    pub output: &'a Path,
    pub lookup_column: &'a str,
    pub registry: ApplyRegistryReference,
    pub resolutions: &'a BTreeMap<String, ApplyCanonicalResolution>,
    pub target_rows_per_chunk: u64,
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
    pub summary: BTreeMap<String, u64>,
    pub streaming: ApplyStreamingDiagnostics,
    pub output_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyInputFormat {
    Csv(u8),
    Jsonl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApplyInputInspection {
    row_count: u64,
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
    let format = apply_input_format(request.rows)?;
    let inspection = inspect_apply_input(request.rows, request.lookup_column, format)?;
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

    let write_result = write_apply_output(&request, format, &chunks)?;
    debug_assert_eq!(write_result.row_count, inspection.row_count);

    let mut artifact = ApplyRunArtifact {
        version: CANON_ENTITY_APPLY_VERSION.to_string(),
        artifact_content_hash: String::new(),
        registry: request.registry.clone(),
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
    format: ApplyInputFormat,
) -> Result<ApplyInputInspection, Refusal> {
    match format {
        ApplyInputFormat::Csv(delimiter) => inspect_apply_csv(rows, lookup_column, delimiter),
        ApplyInputFormat::Jsonl => inspect_apply_jsonl(rows, lookup_column),
    }
}

fn inspect_apply_csv(
    rows: &Path,
    lookup_column: &str,
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
    let _lookup_index = csv_lookup_index(&headers, lookup_column)?;
    validate_no_canonical_csv_headers(&headers)?;

    let mut row_count = 0u64;
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
        let _record = parse_csv_record(&line, delimiter, row_number, "row")?;
        row_count += 1;
    }

    Ok(ApplyInputInspection { row_count })
}

fn inspect_apply_jsonl(rows: &Path, lookup_column: &str) -> Result<ApplyInputInspection, Refusal> {
    let mut reader = open_apply_reader(rows)?;
    let mut line = Vec::new();
    let mut row_count = 0u64;

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
        if !object.contains_key(lookup_column) {
            return Err(input_contract_refusal(
                "Apply JSONL row is missing the lookup column",
                row_number,
                lookup_column,
                "missing field".to_string(),
            ));
        }
        validate_no_canonical_json_fields(&object, row_number)?;
        row_count += 1;
    }

    Ok(ApplyInputInspection { row_count })
}

fn write_apply_output(
    request: &ApplyStreamRequest<'_>,
    format: ApplyInputFormat,
    chunks: &[EntityStreamChunkMetadata],
) -> Result<ApplyWriteResult, Refusal> {
    if let Some(parent) = request.output.parent()
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
    let mut writer = File::create(request.output).map_err(|error| {
        io_budget_refusal(
            "Failed to create apply output",
            request.output,
            error.to_string(),
        )
    })?;

    match format {
        ApplyInputFormat::Csv(delimiter) => write_apply_csv(
            request.rows,
            request.lookup_column,
            request.resolutions,
            delimiter,
            chunks,
            &mut writer,
        ),
        ApplyInputFormat::Jsonl => write_apply_jsonl(
            request.rows,
            request.lookup_column,
            request.resolutions,
            chunks,
            &mut writer,
        ),
    }
}

fn write_apply_csv<W: Write>(
    rows: &Path,
    lookup_column: &str,
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
    write_csv_line_with_appended_fields(writer, &line, delimiter, APPLY_CANONICAL_FIELDS)?;

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
        let lookup_value = record.get(lookup_index).unwrap_or("").trim();
        let resolution = resolutions.get(lookup_value);
        let fields = resolution
            .map(ApplyCanonicalResolution::fields)
            .unwrap_or(["", "", ""]);
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
        let appended = json_line_with_appended_fields(&line, resolution, object.is_empty())?;
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

fn write_csv_line_with_appended_fields<W: Write>(
    writer: &mut W,
    line: &[u8],
    delimiter: u8,
    fields: &[&str],
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

fn encode_csv_fields(fields: &[&str], delimiter: u8) -> Result<Vec<u8>, Refusal> {
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
    resolution: Option<&ApplyCanonicalResolution>,
    object_is_empty: bool,
) -> Result<Vec<u8>, Refusal> {
    let (body_end, close_index) = json_object_close_index(line)?;
    let mut output = Vec::with_capacity(line.len() + 96);
    output.extend_from_slice(&line[..close_index]);
    if !object_is_empty {
        output.extend_from_slice(b",");
    }
    let fields = json_canonical_fields(resolution)?;
    output.extend_from_slice(fields.as_bytes());
    output.extend_from_slice(&line[close_index..body_end]);
    output.extend_from_slice(&line[body_end..]);
    Ok(output)
}

fn json_canonical_fields(resolution: Option<&ApplyCanonicalResolution>) -> Result<String, Refusal> {
    let mut fields = Vec::with_capacity(APPLY_CANONICAL_FIELDS.len());
    match resolution {
        Some(resolution) => {
            fields.push(json_field("canonical_id", Some(&resolution.canonical_id))?);
            fields.push(json_field(
                "canonical_type",
                Some(&resolution.canonical_type),
            )?);
            fields.push(json_field("canonical_rule_id", Some(&resolution.rule_id))?);
        }
        None => {
            fields.push(json_field("canonical_id", None)?);
            fields.push(json_field("canonical_type", None)?);
            fields.push(json_field("canonical_rule_id", None)?);
        }
    }
    Ok(fields.join(","))
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
        Value::String(text) => Ok(text.trim().to_string()),
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

fn io_budget_refusal(message: &str, path: &Path, error: String) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        message,
        json!({ "path": path.display().to_string(), "error": error }),
        None,
    )
}

fn input_contract_refusal(
    message: impl Into<String>,
    row_number: u64,
    field: &str,
    error: String,
) -> Refusal {
    EntityRefusalKind::InputContract.to_refusal(
        message,
        json!({ "row_number": row_number, "field": field, "error": error }),
        None,
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
