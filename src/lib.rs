#![forbid(unsafe_code)]

pub mod cli;
pub mod input;
pub mod lookup;
pub mod output;
pub mod refusal;
pub mod registry;
pub mod witness;

use crate::cli::Cli;
use serde::{Deserialize, Serialize, Serializer};
use std::{
    collections::HashMap,
    error::Error,
    io::Write,
    path::{Path, PathBuf},
};

// Entry point function
pub fn run(cli: Cli) -> Result<u8, Box<dyn Error>> {
    // Step 1: Handle info commands (early return)
    if cli.version {
        println!("canon {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }

    if cli.describe {
        const OPERATOR_JSON: &str = include_str!("../operator.json");
        println!("{OPERATOR_JSON}");
        return Ok(0);
    }

    if cli.schema {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://canon.v0/schema.json",
            "title": "Canon Output Schema",
            "description": "JSON schema for canon.v0 output format",
            "type": "object",
            "required": ["version", "outcome"],
            "properties": {
                "version": {
                    "type": "string",
                    "const": "canon.v0"
                },
                "outcome": {
                    "type": "string",
                    "enum": ["RESOLVED", "PARTIAL", "UNRESOLVED", "REFUSAL"]
                },
                "registry": {
                    "type": ["object", "null"],
                    "properties": {
                        "id": { "type": "string" },
                        "version": { "type": "string" },
                        "source": { "type": "string" }
                    },
                    "required": ["id", "version", "source"]
                },
                "summary": {
                    "type": ["object", "null"],
                    "properties": {
                        "total": { "type": "integer", "minimum": 0 },
                        "resolved": { "type": "integer", "minimum": 0 },
                        "unresolved": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["total", "resolved", "unresolved"]
                },
                "mappings": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "input": { "type": "string" },
                            "canonical_id": { "type": "string" },
                            "canonical_type": { "type": "string" },
                            "rule_id": { "type": "string" },
                            "confidence": { "type": "string" }
                        },
                        "required": ["input", "canonical_id", "canonical_type", "rule_id", "confidence"]
                    }
                },
                "unresolved": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "input": { "type": ["string", "null"] },
                            "reason": { "type": "string" }
                        },
                        "required": ["reason"]
                    }
                },
                "refusal": {
                    "type": ["object", "null"],
                    "properties": {
                        "code": { "type": "string" },
                        "message": { "type": "string" },
                        "detail": { "type": "object" },
                        "next_command": { "type": ["string", "null"] }
                    },
                    "required": ["code", "message", "detail"]
                }
            }
        });
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(0);
    }

    // Step 2: Validate required args
    let input_path = cli
        .input
        .as_ref()
        .ok_or("Input path required")?
        .to_path_buf();
    let registry_path = cli
        .registry
        .as_ref()
        .ok_or("Registry path required")?
        .to_path_buf();
    let column = cli
        .column
        .as_deref()
        .ok_or("Column name required")?
        .to_string();

    // Step 3: Warn on stderr if --map-out is set with --emit json
    if matches!(cli.emit, crate::cli::EmitMode::Json) && cli.map_out.is_some() {
        eprintln!("Warning: --map-out ignored in JSON mode (mapping already is stdout)");
    }

    // Handle refusals by converting to CanonOutput and routing appropriately
    let result = run_pipeline(&input_path, &registry_path, &column, &cli);

    match result {
        Ok(exit_code) => Ok(exit_code),
        Err(refusal_output) => {
            match cli.emit {
                crate::cli::EmitMode::Json => {
                    // Refusal JSON to stdout
                    println!("{}", serde_json::to_string(&refusal_output)?);
                }
                crate::cli::EmitMode::Csv => {
                    // Refusal JSON to stderr
                    eprintln!("{}", serde_json::to_string(&refusal_output)?);
                }
            }
            Ok(2) // REFUSAL exit code
        }
    }
}

// Internal pipeline that can return refusals
#[allow(clippy::result_large_err)]
fn run_pipeline(
    input_path: &Path,
    registry_path: &Path,
    column: &str,
    cli: &Cli,
) -> Result<u8, CanonOutput> {
    // Step 4: Load registry
    let registry = registry::load_registry(registry_path).map_err(create_registry_refusal)?;
    let input_path_display = input_path.to_string_lossy().into_owned();

    // Step 5: Parse input
    let input_values = input::parse_input(input_path, column, cli.max_bytes, cli.max_rows)
        .map_err(create_input_refusal)?;

    // Step 6: Hash input bytes for witness
    let (input_hash, input_bytes) = if input_path == Path::new("-") {
        let hash = input_values.source_hash.clone().ok_or_else(|| {
            create_io_refusal(std::io::Error::other(
                "Failed to hash stdin input bytes during parsing",
            ))
        })?;
        (hash, input_values.source_bytes)
    } else {
        let hash = witness::hash_file(input_path).map_err(|e| {
            create_io_refusal(std::io::Error::other(format!(
                "Failed to hash input file: {}",
                e
            )))
        })?;
        (hash, input_size(input_path))
    };

    // Step 7: Validate emit mode
    if matches!(cli.emit, crate::cli::EmitMode::Csv)
        && matches!(input_values.format, InputFormat::Jsonl)
    {
        return Err(refusal::create_refusal(
            RefusalCode::EEmitFormat,
            "--emit csv cannot be used with JSONL input".to_string(),
            serde_json::json!({"input_format": "jsonl", "emit_mode": "csv"}),
            Some("Use --emit json with JSONL input".to_string()),
        ));
    }

    // Step 8: Resolve values
    let resolve_result =
        lookup::resolve_values(&registry, &input_values).map_err(create_lookup_refusal)?;

    // Step 9: Determine outcome
    let outcome = determine_outcome(&resolve_result.summary);

    // Debug assert for safety net
    debug_assert!(
        !(resolve_result.summary.resolved == 0 && resolve_result.summary.unresolved == 0),
        "Empty input should have been caught by input module"
    );

    // Step 10: Emit output
    let output_hash = match cli.emit {
        crate::cli::EmitMode::Json => {
            // JSON mode: emit to stdout with hash
            let json_output =
                output::json::emit_json_explicit(&registry.meta, &resolve_result, cli.explicit)
                    .map_err(create_output_refusal)?;

            print!("{}", json_output);

            // Step 11: Hash output bytes (witness protocol)
            witness::hash_bytes(json_output.as_bytes())
        }
        crate::cli::EmitMode::Csv => {
            // CSV mode: create resolve map and emit with hash
            let resolve_map = build_resolve_map(&resolve_result);
            let default_canonical_column = format!("{}__canon", column);
            let canonical_column = cli
                .canon_column
                .as_deref()
                .unwrap_or(default_canonical_column.as_str());

            let stdout = std::io::stdout();
            let mut stdout_lock = stdout.lock();
            let mut tee_writer = HashingWriter::new(&mut stdout_lock);
            output::csv::emit_csv(
                input_path,
                &resolve_map,
                column,
                canonical_column,
                input_values.delimiter.unwrap_or(b','),
                &mut tee_writer,
            )
            .map_err(create_csv_output_refusal)?;

            tee_writer.flush().map_err(create_io_refusal)?;

            // Write --map-out sidecar if specified
            if let Some(map_out_path) = &cli.map_out {
                let json_output =
                    output::json::emit_json_explicit(&registry.meta, &resolve_result, cli.explicit)
                        .map_err(create_output_refusal)?;
                std::fs::write(map_out_path, json_output).map_err(create_io_refusal)?;
            }

            // Step 11: Hash output bytes (witness protocol)
            tee_writer.finalize_hash()
        }
    };

    // Step 12: Record witness (unless --no-witness)
    let exit_code = match outcome {
        Outcome::Resolved => 0,
        Outcome::Partial | Outcome::Unresolved => 1,
        Outcome::Refusal => 2, // Should not reach here
    };

    let no_witness = cli.no_witness;
    if !no_witness {
        let witness_summary = witness::WitnessSummary {
            total: resolve_result.summary.total,
            resolved: resolve_result.summary.resolved,
            unresolved: resolve_result.summary.unresolved,
        };

        let outcome_str = match outcome {
            Outcome::Resolved => "RESOLVED",
            Outcome::Partial => "PARTIAL",
            Outcome::Unresolved => "UNRESOLVED",
            Outcome::Refusal => "REFUSAL",
        };

        let mut params = serde_json::Map::new();
        params.insert(
            "input_path".to_string(),
            serde_json::Value::String(input_path_display.clone()),
        );
        params.insert(
            "registry_id".to_string(),
            serde_json::Value::String(registry.meta.id.clone()),
        );
        params.insert(
            "registry_version".to_string(),
            serde_json::Value::String(registry.meta.version.clone()),
        );
        params.insert(
            "column".to_string(),
            serde_json::Value::String(column.to_string()),
        );
        params.insert(
            "emit".to_string(),
            serde_json::Value::String(
                match cli.emit {
                    crate::cli::EmitMode::Json => "json",
                    crate::cli::EmitMode::Csv => "csv",
                }
                .to_string(),
            ),
        );
        params.insert(
            "explicit".to_string(),
            serde_json::Value::Bool(cli.explicit),
        );
        params.insert(
            "summary".to_string(),
            serde_json::json!({
                "total": witness_summary.total,
                "resolved": witness_summary.resolved,
                "unresolved": witness_summary.unresolved
            }),
        );
        if let Some(canon_column) = &cli.canon_column {
            params.insert(
                "canon_column".to_string(),
                serde_json::Value::String(canon_column.clone()),
            );
        }
        if let Some(map_out) = &cli.map_out {
            params.insert(
                "map_out".to_string(),
                serde_json::Value::String(map_out.display().to_string()),
            );
        }
        if let Some(max_rows) = cli.max_rows {
            params.insert(
                "max_rows".to_string(),
                serde_json::Value::from(max_rows as u64),
            );
        }
        if let Some(max_bytes) = cli.max_bytes {
            params.insert("max_bytes".to_string(), serde_json::Value::from(max_bytes));
        }

        let witness_record = witness::WitnessRecord::new(
            vec![witness::WitnessInput {
                path: input_path_display.clone(),
                hash: Some(input_hash.clone()),
                bytes: input_bytes,
            }],
            params,
            &output_hash,
            outcome_str,
            exit_code,
        );

        if let Err(error) = witness::append_witness_record(&witness_record, no_witness) {
            eprintln!("Warning: failed to append witness: {}", error);
        }
    }

    // Step 13: Return exit code
    Ok(exit_code)
}

fn determine_outcome(summary: &Summary) -> Outcome {
    match (summary.resolved, summary.unresolved) {
        (resolved, 0) if resolved > 0 => Outcome::Resolved,
        (resolved, unresolved) if resolved > 0 && unresolved > 0 => Outcome::Partial,
        (0, unresolved) if unresolved > 0 => Outcome::Unresolved,
        _ => {
            debug_assert!(false, "Invalid summary state");
            Outcome::Unresolved
        }
    }
}

fn build_resolve_map(
    resolve_result: &ResolveResult,
) -> std::collections::HashMap<String, Option<String>> {
    let mut resolve_map = std::collections::HashMap::new();

    // Add resolved mappings
    for mapping in &resolve_result.mappings {
        resolve_map.insert(mapping.input.clone(), Some(mapping.canonical_id.clone()));
    }

    // Add unresolved entries that have input values
    for unresolved in &resolve_result.unresolved {
        if let Some(input_value) = &unresolved.input {
            resolve_map.insert(input_value.clone(), None);
        }
    }

    resolve_map
}

fn input_size(path: &Path) -> Option<u64> {
    if path == Path::new("-") {
        return None;
    }

    std::fs::metadata(path).ok().map(|metadata| metadata.len())
}

struct HashingWriter<W: Write> {
    writer: W,
    hasher: blake3::Hasher,
}

impl<W: Write> HashingWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            hasher: blake3::Hasher::new(),
        }
    }

    fn finalize_hash(self) -> String {
        format!("blake3:{}", self.hasher.finalize().to_hex())
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let bytes_written = self.writer.write(buf)?;
        self.hasher.update(&buf[..bytes_written]);
        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

// Helper functions to create refusal outputs from errors
fn create_registry_refusal(error: Box<dyn Error>) -> CanonOutput {
    let message = error.to_string();
    let code = if message.contains("Registry directory not found") {
        RefusalCode::EIo
    } else {
        RefusalCode::EBadRegistry
    };
    refusal::create_refusal(code, message, serde_json::json!({}), None)
}

fn create_input_refusal(error: input::InputError) -> CanonOutput {
    match error {
        input::InputError::ColumnNotFound { column, available } => refusal::create_refusal(
            RefusalCode::EColumnNotFound,
            format!("Column '{}' not found in input file", column),
            serde_json::json!({
                "column": column,
                "available_columns": available
            }),
            None,
        ),
        input::InputError::TooLarge {
            limit_type,
            limit,
            actual,
        } => refusal::create_refusal(
            RefusalCode::ETooLarge,
            format!(
                "Input exceeds --{} limit ({} > {})",
                limit_type, actual, limit
            ),
            serde_json::json!({
                "limit_type": limit_type,
                "limit": limit,
                "actual": actual
            }),
            None,
        ),
        input::InputError::Io(message) => {
            refusal::create_refusal(RefusalCode::EIo, message, serde_json::json!({}), None)
        }
        input::InputError::Parse(message) => {
            refusal::create_refusal(RefusalCode::EParse, message, serde_json::json!({}), None)
        }
        input::InputError::CsvParse(message) => {
            refusal::create_refusal(RefusalCode::ECsvParse, message, serde_json::json!({}), None)
        }
        input::InputError::Encoding(message) => {
            refusal::create_refusal(RefusalCode::EEncoding, message, serde_json::json!({}), None)
        }
        input::InputError::EmptyInput => refusal::create_refusal(
            RefusalCode::EEmptyInput,
            "Input has no processable rows".to_string(),
            serde_json::json!({}),
            None,
        ),
    }
}

fn create_lookup_refusal(error: lookup::LookupError) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EIo,
        error.to_string(),
        serde_json::json!({}),
        None,
    )
}

fn create_output_refusal(error: Box<dyn Error>) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EIo,
        error.to_string(),
        serde_json::json!({}),
        None,
    )
}

fn create_csv_output_refusal(error: output::csv::CsvOutputError) -> CanonOutput {
    match error {
        output::csv::CsvOutputError::Io(message) => {
            refusal::create_refusal(RefusalCode::EIo, message, serde_json::json!({}), None)
        }
        output::csv::CsvOutputError::CsvParse(message) => {
            refusal::create_refusal(RefusalCode::ECsvParse, message, serde_json::json!({}), None)
        }
        output::csv::CsvOutputError::ColumnExists { column } => refusal::create_refusal(
            RefusalCode::EColumnExists,
            format!("Canonical column '{}' already exists in CSV header", column),
            serde_json::json!({ "canon_column": column }),
            None,
        ),
        output::csv::CsvOutputError::ColumnNotFound { column, available } => {
            refusal::create_refusal(
                RefusalCode::EColumnNotFound,
                format!("Column '{}' not found", column),
                serde_json::json!({ "column": column, "available_columns": available }),
                None,
            )
        }
    }
}

fn create_io_refusal(error: std::io::Error) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EIo,
        error.to_string(),
        serde_json::json!({}),
        None,
    )
}

// Output types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    Resolved,
    Partial,
    Unresolved,
    Refusal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    // NOTE: input and canonical_id are RAW values without u8:/hex: prefix
    // JSON output applies encoding at serialization time
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnresolvedEntry {
    // input is RAW or None for special reasons
    pub input: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryMeta {
    pub id: String,
    pub version: String,
    // source is CLI arg verbatim
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub total: usize,
    pub resolved: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonOutput {
    pub version: String,
    pub outcome: Outcome,
    pub registry: Option<RegistryMeta>,
    pub summary: Option<Summary>,
    pub mappings: Vec<Mapping>,
    pub unresolved: Vec<UnresolvedEntry>,
    pub refusal: Option<Refusal>,
}

// Refusal types
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum RefusalCode {
    EIo,
    EEncoding,
    ECsvParse,
    EBadRegistry,
    EColumnNotFound,
    EParse,
    EEmptyInput,
    ETooLarge,
    EEmitFormat,
    EColumnExists,
}

impl Serialize for RefusalCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let code_str = match self {
            RefusalCode::EIo => "E_IO",
            RefusalCode::EEncoding => "E_ENCODING",
            RefusalCode::ECsvParse => "E_CSV_PARSE",
            RefusalCode::EBadRegistry => "E_BAD_REGISTRY",
            RefusalCode::EColumnNotFound => "E_COLUMN_NOT_FOUND",
            RefusalCode::EParse => "E_PARSE",
            RefusalCode::EEmptyInput => "E_EMPTY_INPUT",
            RefusalCode::ETooLarge => "E_TOO_LARGE",
            RefusalCode::EEmitFormat => "E_EMIT_FORMAT",
            RefusalCode::EColumnExists => "E_COLUMN_EXISTS",
        };
        serializer.serialize_str(code_str)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub code: RefusalCode,
    pub message: String,
    pub detail: serde_json::Value,
    pub next_command: Option<String>,
}

// Cross-module types (the 8-agent contract)
#[derive(Debug, Clone, PartialEq)]
pub enum InputFormat {
    Csv,
    Jsonl,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SpecialReason {
    EmptyValue,
    NullValue,
    MissingField,
    NonScalarValue,
}

impl std::fmt::Display for SpecialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason_str = match self {
            SpecialReason::EmptyValue => "empty_value",
            SpecialReason::NullValue => "null_value",
            SpecialReason::MissingField => "missing_field",
            SpecialReason::NonScalarValue => "non_scalar_value",
        };
        write!(f, "{}", reason_str)
    }
}

#[derive(Debug, Clone)]
pub struct InputValues {
    pub values: HashMap<String, ()>,
    pub special: HashMap<SpecialReason, usize>,
    pub format: InputFormat,
    pub delimiter: Option<u8>,
    pub source_hash: Option<String>,
    pub source_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Registry {
    pub meta: RegistryMeta,
    pub db_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ResolveResult {
    pub mappings: Vec<Mapping>,
    pub unresolved: Vec<UnresolvedEntry>,
    pub summary: Summary,
}
