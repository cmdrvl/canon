use crate::{
    InputFormat, Refusal,
    input::{self, InputError},
    strategy_registry::{StrategyColumn, StrategySchemaShape},
};
use csv::{ReaderBuilder, StringRecord};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, BufRead, BufReader, Read},
    path::Path,
};

type ProfileResult<T> = Result<T, Refusal>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProfileInput {
    pub source: String,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    pub bytes: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProfileSummary {
    pub rows: u64,
    pub columns: usize,
    pub scalar_values: u64,
    pub null_values: u64,
    pub empty_values: u64,
    pub missing_values: u64,
    pub non_scalar_values: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPrimitiveCounts {
    pub boolean: u64,
    pub integer: u64,
    pub number: u64,
    pub string: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProfileColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub cardinality: u64,
    pub cardinality_exact: bool,
    pub value_count: u64,
    pub null_count: u64,
    pub empty_count: u64,
    pub missing_count: u64,
    pub non_scalar_count: u64,
    pub primitive_counts: StrategyPrimitiveCounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProfileOutput {
    pub version: String,
    pub input: StrategyProfileInput,
    pub summary: StrategyProfileSummary,
    pub schema_fingerprint: String,
    pub profile_content_hash: String,
    pub columns: Vec<StrategyProfileColumn>,
}

impl StrategyProfileOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{} rows={} columns={} schema={} profile={}",
            self.input.source,
            self.summary.rows,
            self.summary.columns,
            self.schema_fingerprint,
            self.profile_content_hash,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct StrategyProfileContent<'a> {
    version: &'a str,
    summary: &'a StrategyProfileSummary,
    schema_fingerprint: &'a str,
    columns: &'a [StrategyProfileColumn],
}

#[derive(Debug, Default)]
struct ColumnStats {
    name: String,
    value_count: u64,
    null_count: u64,
    empty_count: u64,
    missing_count: u64,
    non_scalar_count: u64,
    distinct_values: BTreeSet<String>,
    primitive_counts: PrimitiveCounts,
}

impl ColumnStats {
    fn new(name: String) -> Self {
        Self {
            name,
            ..Self::default()
        }
    }

    fn observe_scalar(&mut self, rendered: String, primitive: PrimitiveKind) {
        self.value_count += 1;
        self.distinct_values.insert(rendered);
        self.primitive_counts.increment(primitive);
    }

    fn to_profile_column(&self) -> StrategyProfileColumn {
        StrategyProfileColumn {
            name: self.name.clone(),
            kind: self.detected_type().to_string(),
            cardinality: self.distinct_values.len() as u64,
            cardinality_exact: true,
            value_count: self.value_count,
            null_count: self.null_count,
            empty_count: self.empty_count,
            missing_count: self.missing_count,
            non_scalar_count: self.non_scalar_count,
            primitive_counts: StrategyPrimitiveCounts {
                boolean: self.primitive_counts.boolean,
                integer: self.primitive_counts.integer,
                number: self.primitive_counts.number,
                string: self.primitive_counts.string,
            },
        }
    }

    fn detected_type(&self) -> &'static str {
        let has_boolean = self.primitive_counts.boolean > 0;
        let has_integer = self.primitive_counts.integer > 0;
        let has_number = self.primitive_counts.number > 0;
        let has_string = self.primitive_counts.string > 0;
        let observed_types =
            has_boolean as u8 + has_integer as u8 + has_number as u8 + has_string as u8;

        match observed_types {
            0 => "empty",
            1 if has_boolean => "boolean",
            1 if has_integer => "integer",
            1 if has_number => "number",
            1 if has_string => "string",
            2 if has_integer && has_number => "number",
            _ => "mixed",
        }
    }
}

#[derive(Debug, Default)]
struct PrimitiveCounts {
    boolean: u64,
    integer: u64,
    number: u64,
    string: u64,
}

impl PrimitiveCounts {
    fn increment(&mut self, primitive: PrimitiveKind) {
        match primitive {
            PrimitiveKind::Boolean => self.boolean += 1,
            PrimitiveKind::Integer => self.integer += 1,
            PrimitiveKind::Number => self.number += 1,
            PrimitiveKind::String => self.string += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimitiveKind {
    Boolean,
    Integer,
    Number,
    String,
}

#[derive(Debug)]
struct HashingReader<R> {
    inner: R,
    hasher: blake3::Hasher,
    bytes: u64,
}

impl<R> HashingReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
            bytes: 0,
        }
    }

    fn content_hash(self) -> (u64, String) {
        (
            self.bytes,
            format!("blake3:{}", self.hasher.finalize().to_hex()),
        )
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_read = self.inner.read(buf)?;
        if bytes_read > 0 {
            self.bytes += bytes_read as u64;
            self.hasher.update(&buf[..bytes_read]);
        }
        Ok(bytes_read)
    }
}

pub fn profile(
    input_path: &Path,
    max_bytes: Option<u64>,
    max_rows: Option<usize>,
) -> ProfileResult<StrategyProfileOutput> {
    let format =
        input::detect_format(input_path).map_err(|error| input_refusal(input_path, error))?;

    if input_path != Path::new("-")
        && let Some(limit) = max_bytes
    {
        let file_size = std::fs::metadata(input_path)
            .map_err(|error| {
                Refusal::io_error(&input_path.display().to_string(), &error.to_string())
            })?
            .len();
        if file_size > limit {
            return Err(Refusal::too_large(
                "max_bytes",
                &limit.to_string(),
                &file_size.to_string(),
            ));
        }
    }

    match format {
        InputFormat::Csv => profile_csv(input_path, max_rows),
        InputFormat::Jsonl => profile_jsonl(input_path, max_bytes, max_rows),
    }
}

fn profile_csv(input_path: &Path, max_rows: Option<usize>) -> ProfileResult<StrategyProfileOutput> {
    let delimiter_file = File::open(input_path).map_err(|error| {
        Refusal::io_error(&input_path.display().to_string(), &error.to_string())
    })?;
    let delimiter = input::detect_csv_delimiter(&delimiter_file)
        .map_err(|error| input_refusal(input_path, error))?;
    let file = File::open(input_path).map_err(|error| {
        Refusal::io_error(&input_path.display().to_string(), &error.to_string())
    })?;
    let hashing_reader = HashingReader::new(file);
    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(hashing_reader);

    let headers = reader
        .headers()
        .map_err(|error| Refusal::csv_parse(&input_path.display().to_string(), &error.to_string()))?
        .clone();
    let (header_names, mut stats) = stats_from_headers(&headers, input_path, 0)?;
    let mut row_count = 0u64;

    for record in reader.records() {
        let record = record.map_err(|error| {
            Refusal::csv_parse(&input_path.display().to_string(), &error.to_string())
        })?;
        if is_blank_record(&record) {
            continue;
        }
        enforce_max_rows(row_count, max_rows)?;
        row_count += 1;

        for (index, name) in header_names.iter().enumerate() {
            let column = stats
                .get_mut(name)
                .expect("header names and stats are built together");
            match record.get(index) {
                Some(raw) => observe_text_cell(column, raw),
                None => column.missing_count += 1,
            }
        }
    }

    if row_count == 0 {
        return Err(Refusal::empty_input(&input_path.display().to_string()));
    }

    let (bytes, input_hash) = reader.into_inner().content_hash();
    Ok(build_output(
        input_path,
        format_label(input_path, InputFormat::Csv, Some(delimiter)),
        Some(render_delimiter(delimiter)),
        row_count,
        bytes,
        input_hash,
        stats,
    ))
}

fn profile_jsonl(
    input_path: &Path,
    max_bytes: Option<u64>,
    max_rows: Option<usize>,
) -> ProfileResult<StrategyProfileOutput> {
    if input_path == Path::new("-") {
        let stdin = io::stdin();
        profile_jsonl_reader(input_path, stdin.lock(), max_bytes, max_rows)
    } else {
        let file = File::open(input_path).map_err(|error| {
            Refusal::io_error(&input_path.display().to_string(), &error.to_string())
        })?;
        profile_jsonl_reader(input_path, BufReader::new(file), max_bytes, max_rows)
    }
}

fn profile_jsonl_reader<R: BufRead>(
    input_path: &Path,
    mut reader: R,
    max_bytes: Option<u64>,
    max_rows: Option<usize>,
) -> ProfileResult<StrategyProfileOutput> {
    let mut stats = BTreeMap::<String, ColumnStats>::new();
    let mut row_count = 0u64;
    let mut byte_count = 0u64;
    let mut hasher = blake3::Hasher::new();
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).map_err(|error| {
            Refusal::io_error(&input_path.display().to_string(), &error.to_string())
        })?;
        if bytes_read == 0 {
            break;
        }

        byte_count += bytes_read as u64;
        hasher.update(line.as_bytes());
        if let Some(limit) = max_bytes
            && byte_count > limit
        {
            return Err(Refusal::too_large(
                "max_bytes",
                &limit.to_string(),
                &byte_count.to_string(),
            ));
        }

        if line.trim().is_empty() {
            continue;
        }
        enforce_max_rows(row_count, max_rows)?;
        row_count += 1;

        let value: Value = serde_json::from_str(&line).map_err(|error| {
            Refusal::parse_error(
                &input_path.display().to_string(),
                &format!("Invalid JSON on line {}: {}", row_count, error),
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            Refusal::parse_error(
                &input_path.display().to_string(),
                &format!("Line {} is not a JSON object", row_count),
            )
        })?;

        let previous_rows = row_count - 1;
        let mut row_values = BTreeMap::new();
        for (key, value) in object {
            let canonical_name = canonical_column_name(key).map_err(|message| {
                Refusal::parse_error(&input_path.display().to_string(), &message)
            })?;
            if row_values.insert(canonical_name, value).is_some() {
                return Err(Refusal::parse_error(
                    &input_path.display().to_string(),
                    &format!("Line {row_count} contains duplicate field names after trimming"),
                ));
            }
        }

        for canonical_name in row_values.keys() {
            stats.entry(canonical_name.clone()).or_insert_with(|| {
                let mut column = ColumnStats::new(canonical_name.clone());
                column.missing_count = previous_rows;
                column
            });
        }

        for column in stats.values_mut() {
            match row_values.get(&column.name) {
                Some(value) => observe_json_value(column, value),
                None => column.missing_count += 1,
            }
        }
    }

    if row_count == 0 {
        return Err(Refusal::empty_input(&input_path.display().to_string()));
    }

    Ok(build_output(
        input_path,
        format_label(input_path, InputFormat::Jsonl, None),
        None,
        row_count,
        byte_count,
        format!("blake3:{}", hasher.finalize().to_hex()),
        stats,
    ))
}

fn stats_from_headers(
    headers: &StringRecord,
    input_path: &Path,
    previous_rows: u64,
) -> ProfileResult<(Vec<String>, BTreeMap<String, ColumnStats>)> {
    let mut header_names = Vec::new();
    let mut stats = BTreeMap::new();
    for (index, raw_name) in headers.iter().enumerate() {
        let name = canonical_column_name(raw_name).map_err(|message| {
            Refusal::parse_error(
                &input_path.display().to_string(),
                &format!("CSV header column {index}: {message}"),
            )
        })?;
        if stats.contains_key(&name) {
            return Err(Refusal::parse_error(
                &input_path.display().to_string(),
                &format!("duplicate column '{}'", name),
            ));
        }
        let mut column = ColumnStats::new(name.clone());
        column.missing_count = previous_rows;
        stats.insert(name, column);
        header_names.push(raw_name.trim().to_string());
    }
    if stats.is_empty() {
        return Err(Refusal::parse_error(
            &input_path.display().to_string(),
            "CSV input must contain at least one header column",
        ));
    }
    Ok((header_names, stats))
}

fn build_output(
    input_path: &Path,
    format: String,
    delimiter: Option<String>,
    row_count: u64,
    bytes: u64,
    input_hash: String,
    stats: BTreeMap<String, ColumnStats>,
) -> StrategyProfileOutput {
    let columns = stats
        .values()
        .map(ColumnStats::to_profile_column)
        .collect::<Vec<_>>();
    let summary = StrategyProfileSummary {
        rows: row_count,
        columns: columns.len(),
        scalar_values: columns.iter().map(|column| column.value_count).sum(),
        null_values: columns.iter().map(|column| column.null_count).sum(),
        empty_values: columns.iter().map(|column| column.empty_count).sum(),
        missing_values: columns.iter().map(|column| column.missing_count).sum(),
        non_scalar_values: columns.iter().map(|column| column.non_scalar_count).sum(),
    };
    let schema = StrategySchemaShape {
        columns: columns
            .iter()
            .map(|column| StrategyColumn {
                name: column.name.clone(),
                kind: column.kind.clone(),
                cardinality: Some(column.cardinality),
            })
            .collect(),
    };
    let schema_fingerprint = hash_json(&schema);
    let version = "canon_strategy_profile.v0";
    let content = StrategyProfileContent {
        version,
        summary: &summary,
        schema_fingerprint: &schema_fingerprint,
        columns: &columns,
    };
    let profile_content_hash = hash_json(&content);

    StrategyProfileOutput {
        version: version.to_string(),
        input: StrategyProfileInput {
            source: input_path.display().to_string(),
            format,
            delimiter,
            bytes,
            content_hash: input_hash,
        },
        summary,
        schema_fingerprint,
        profile_content_hash,
        columns,
    }
}

fn observe_text_cell(column: &mut ColumnStats, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        column.empty_count += 1;
    } else {
        column.observe_scalar(trimmed.to_string(), classify_text_primitive(trimmed));
    }
}

fn observe_json_value(column: &mut ColumnStats, value: &Value) {
    match value {
        Value::Null => column.null_count += 1,
        Value::Array(_) | Value::Object(_) => column.non_scalar_count += 1,
        Value::Bool(value) => {
            column.observe_scalar(value.to_string(), PrimitiveKind::Boolean);
        }
        Value::Number(value) => {
            let primitive = if value.as_i64().is_some() || value.as_u64().is_some() {
                PrimitiveKind::Integer
            } else {
                PrimitiveKind::Number
            };
            column.observe_scalar(value.to_string(), primitive);
        }
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                column.empty_count += 1;
            } else {
                column.observe_scalar(trimmed.to_string(), PrimitiveKind::String);
            }
        }
    }
}

fn classify_text_primitive(value: &str) -> PrimitiveKind {
    let lower = value.to_ascii_lowercase();
    if matches!(lower.as_str(), "true" | "false") {
        return PrimitiveKind::Boolean;
    }
    if let Ok(parsed) = value.parse::<i64>()
        && parsed.to_string() == value
    {
        return PrimitiveKind::Integer;
    }
    if let Ok(parsed) = value.parse::<u64>()
        && parsed.to_string() == value
    {
        return PrimitiveKind::Integer;
    }
    if value.parse::<f64>().is_ok_and(f64::is_finite) {
        PrimitiveKind::Number
    } else {
        PrimitiveKind::String
    }
}

fn canonical_column_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        Err("column name cannot be empty".to_string())
    } else {
        Ok(name.to_string())
    }
}

fn enforce_max_rows(row_count: u64, max_rows: Option<usize>) -> ProfileResult<()> {
    if let Some(limit) = max_rows
        && row_count >= limit as u64
    {
        return Err(Refusal::too_large(
            "max_rows",
            &limit.to_string(),
            &row_count.to_string(),
        ));
    }
    Ok(())
}

fn input_refusal(input_path: &Path, error: InputError) -> Refusal {
    match error {
        InputError::Io(message) => Refusal::io_error(&input_path.display().to_string(), &message),
        InputError::Parse(message) => {
            Refusal::parse_error(&input_path.display().to_string(), &message)
        }
        InputError::CsvParse(message) => {
            Refusal::csv_parse(&input_path.display().to_string(), &message)
        }
        InputError::Encoding(message) => {
            Refusal::encoding_error(&input_path.display().to_string(), &message)
        }
        InputError::EmptyInput => Refusal::empty_input(&input_path.display().to_string()),
        InputError::ColumnNotFound { column, available } => {
            Refusal::column_not_found(&column, &available)
        }
        InputError::TooLarge {
            limit_type,
            limit,
            actual,
        } => Refusal::too_large(&limit_type, &limit, &actual),
    }
}

fn is_blank_record(record: &StringRecord) -> bool {
    record.iter().all(|field| field.trim().is_empty())
}

fn format_label(input_path: &Path, format: InputFormat, delimiter: Option<u8>) -> String {
    input_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|extension| matches!(extension.as_str(), "csv" | "tsv" | "jsonl" | "ndjson"))
        .unwrap_or_else(|| match format {
            InputFormat::Csv if delimiter == Some(b'\t') => "tsv".to_string(),
            InputFormat::Csv => "csv".to_string(),
            InputFormat::Jsonl => "jsonl".to_string(),
        })
}

fn render_delimiter(delimiter: u8) -> String {
    match delimiter {
        b'\t' => "\\t".to_string(),
        b',' => ",".to_string(),
        b'|' => "|".to_string(),
        b';' => ";".to_string(),
        other => format!("0x{other:02x}"),
    }
}

fn hash_json<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("serializing profile content is infallible");
    format!("blake3:{}", blake3::hash(&bytes).to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RefusalCode;
    use std::fs;
    use tempfile::NamedTempFile;

    fn temp_file_with_suffix(suffix: &str, content: &str) -> NamedTempFile {
        let file = NamedTempFile::with_suffix(suffix).unwrap();
        fs::write(file.path(), content).unwrap();
        file
    }

    #[test]
    fn profiles_csv_columns_with_sorted_stats_and_hashes() {
        let file = temp_file_with_suffix(
            ".csv",
            "name,age,active\nAlice,42,true\nBob,,false\nAlice,7,true\n",
        );

        let output = profile(file.path(), None, None).unwrap();

        assert_eq!(output.version, "canon_strategy_profile.v0");
        assert_eq!(output.input.format, "csv");
        assert_eq!(output.input.delimiter.as_deref(), Some(","));
        assert_eq!(output.summary.rows, 3);
        assert_eq!(
            output
                .columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            vec!["active", "age", "name"]
        );
        assert_eq!(output.columns[0].kind, "boolean");
        assert_eq!(output.columns[1].kind, "integer");
        assert_eq!(output.columns[1].empty_count, 1);
        assert_eq!(output.columns[1].cardinality, 2);
        assert_eq!(output.columns[2].kind, "string");
        assert_eq!(output.columns[2].cardinality, 2);
        assert!(output.input.content_hash.starts_with("blake3:"));
        assert!(output.profile_content_hash.starts_with("blake3:"));
        assert!(output.schema_fingerprint.starts_with("blake3:"));
    }

    #[test]
    fn profiles_tsv_with_tab_delimiter() {
        let file = temp_file_with_suffix(".tsv", "id\tamount\nA\t1.5\nB\t2.5\n");

        let output = profile(file.path(), None, None).unwrap();

        assert_eq!(output.input.format, "tsv");
        assert_eq!(output.input.delimiter.as_deref(), Some("\\t"));
        assert_eq!(output.columns[0].name, "amount");
        assert_eq!(output.columns[0].kind, "number");
    }

    #[test]
    fn profiles_jsonl_mixed_null_missing_and_non_scalar_fields() {
        let file = temp_file_with_suffix(
            ".jsonl",
            "{\"id\":\"A\",\"score\":1,\"flag\":true}\n{\"id\":\"\",\"score\":null,\"extra\":{\"x\":1}}\n{\"score\":1.5,\"flag\":false}\n",
        );

        let output = profile(file.path(), None, None).unwrap();

        assert_eq!(output.input.format, "jsonl");
        assert_eq!(output.summary.rows, 3);
        let columns = output
            .columns
            .iter()
            .map(|column| (column.name.as_str(), column))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(columns["score"].kind, "number");
        assert_eq!(columns["score"].null_count, 1);
        assert_eq!(columns["score"].cardinality, 2);
        assert_eq!(columns["id"].kind, "string");
        assert_eq!(columns["id"].empty_count, 1);
        assert_eq!(columns["id"].missing_count, 1);
        assert_eq!(columns["extra"].kind, "empty");
        assert_eq!(columns["extra"].missing_count, 2);
        assert_eq!(columns["extra"].non_scalar_count, 1);
        assert_eq!(columns["flag"].kind, "boolean");
        assert_eq!(columns["flag"].missing_count, 1);
    }

    #[test]
    fn profile_output_is_deterministic_for_same_input() {
        let file = temp_file_with_suffix(".csv", "b,a\n2,true\n3,false\n");

        let first = profile(file.path(), None, None).unwrap();
        let second = profile(file.path(), None, None).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn profile_refuses_max_rows_and_max_bytes() {
        let file = temp_file_with_suffix(".jsonl", "{\"id\":\"A\"}\n{\"id\":\"B\"}\n");

        let row_refusal = profile(file.path(), None, Some(1)).unwrap_err();
        assert_eq!(row_refusal.code, RefusalCode::ETooLarge);
        assert_eq!(row_refusal.detail["limit_type"], "max_rows");

        let byte_refusal = profile(file.path(), Some(1), None).unwrap_err();
        assert_eq!(byte_refusal.code, RefusalCode::ETooLarge);
        assert_eq!(byte_refusal.detail["limit_type"], "max_bytes");
    }

    #[test]
    fn profile_columns_can_be_parsed_as_strategy_schema_shape() {
        let file = temp_file_with_suffix(".csv", "vendor,amount\nAcme,10\nBolt,20\n");
        let output = profile(file.path(), None, None).unwrap();
        let value = serde_json::to_value(&output).unwrap();
        let columns = value.get("columns").unwrap();

        assert!(columns.is_array());
        assert_eq!(value["columns"][0]["name"], "amount");
        assert_eq!(value["columns"][0]["type"], "integer");
        assert_eq!(value["columns"][0]["cardinality"], 2);
    }
}
