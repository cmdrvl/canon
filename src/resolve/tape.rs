use super::{
    LoadedTape, LoadedTapes, ResolveError, ResolveErrorCode, ResolveRecord, ResolveResult,
    ResolveStrategy, TapeLoadOptions, TapeSide,
};
use crate::{InputFormat, input};
use csv::ReaderBuilder;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

const COMPOSITE_ID_SEPARATOR: &str = "|";

pub fn load_tapes(
    reference_path: &Path,
    target_path: &Path,
    strategy: &ResolveStrategy,
    options: TapeLoadOptions,
) -> ResolveResult<LoadedTapes> {
    Ok(LoadedTapes {
        reference: load_tape(
            reference_path,
            TapeSide::Reference,
            strategy,
            options.clone(),
        )?,
        target: load_tape(target_path, TapeSide::Target, strategy, options)?,
    })
}

pub fn load_tape(
    path: &Path,
    side: TapeSide,
    strategy: &ResolveStrategy,
    options: TapeLoadOptions,
) -> ResolveResult<LoadedTape> {
    if path == Path::new("-") {
        return Err(input_contract_error(
            "canon resolve tapes must be filesystem paths, not stdin",
            json!({ "side": side_name(side), "path": "-" }),
        ));
    }

    enforce_max_bytes(path, side, options.max_bytes)?;
    let format = input::detect_format(path).map_err(|error| input_error(path, side, error))?;
    let required_columns = required_columns_for_side(strategy, side);

    let loaded = match format {
        InputFormat::Csv => load_csv_tape(path, side, &required_columns, strategy, &options)?,
        InputFormat::Jsonl => load_jsonl_tape(path, side, &required_columns, strategy, &options)?,
    };

    if loaded.records.is_empty() {
        return Err(ResolveError::with_detail(
            ResolveErrorCode::EmptyTape,
            format!(
                "{} tape '{}' contains no processable records",
                side_name(side),
                path.display()
            ),
            json!({
                "side": side_name(side),
                "path": path.display().to_string()
            }),
        ));
    }

    Ok(loaded)
}

pub fn required_columns_for_side(strategy: &ResolveStrategy, side: TapeSide) -> BTreeSet<String> {
    let mut columns = BTreeSet::new();

    match side {
        TapeSide::Reference => {
            columns.extend(strategy.identity.reference.id_columns.iter().cloned())
        }
        TapeSide::Target => columns.extend(strategy.identity.target.id_columns.iter().cloned()),
    }

    for operator in strategy
        .candidate_filter
        .iter()
        .chain(strategy.assertions.iter())
    {
        match side {
            TapeSide::Reference => {
                columns.insert(operator.field_ref.clone());
            }
            TapeSide::Target => {
                columns.insert(operator.field_tgt.clone());
            }
        }
    }

    columns
}

fn load_csv_tape(
    path: &Path,
    side: TapeSide,
    required_columns: &BTreeSet<String>,
    strategy: &ResolveStrategy,
    options: &TapeLoadOptions,
) -> ResolveResult<LoadedTape> {
    let file = File::open(path).map_err(|error| io_error(path, side, error))?;
    let delimiter =
        input::detect_csv_delimiter(&file).map_err(|error| input_error(path, side, error))?;
    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(file);

    let headers = reader
        .headers()
        .map_err(|error| parse_error(path, side, format!("Cannot read CSV headers: {error}")))?
        .iter()
        .map(String::from)
        .collect::<Vec<_>>();
    validate_required_columns(path, side, required_columns, &headers)?;

    let id_columns = id_columns_for_side(strategy, side);
    let mut records = Vec::new();
    let mut seen_ids = BTreeSet::new();

    for result in reader.records() {
        let record =
            result.map_err(|error| parse_error(path, side, format!("CSV parse error: {error}")))?;

        if record.iter().all(|field| field.trim().is_empty()) {
            continue;
        }

        if let Some(limit) = options.max_rows
            && records.len() >= limit
        {
            return Err(too_large_error(
                side,
                "max_rows",
                limit.to_string(),
                (records.len() + 1).to_string(),
            ));
        }

        let row_index = records.len() + 1;
        let mut attributes = BTreeMap::new();
        for (index, header) in headers.iter().enumerate() {
            attributes.insert(
                header.clone(),
                Value::String(record.get(index).unwrap_or("").to_string()),
            );
        }

        push_record(
            side,
            row_index,
            attributes,
            id_columns,
            &mut seen_ids,
            &mut records,
        )?;
    }

    Ok(LoadedTape {
        side,
        path: path.display().to_string(),
        format: InputFormat::Csv,
        delimiter: Some(delimiter),
        records,
    })
}

fn load_jsonl_tape(
    path: &Path,
    side: TapeSide,
    required_columns: &BTreeSet<String>,
    strategy: &ResolveStrategy,
    options: &TapeLoadOptions,
) -> ResolveResult<LoadedTape> {
    let file = File::open(path).map_err(|error| io_error(path, side, error))?;
    let mut reader = BufReader::new(file);
    let id_columns = id_columns_for_side(strategy, side);
    let mut records = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let mut byte_count = 0u64;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| io_error(path, side, error))?;
        if bytes_read == 0 {
            break;
        }

        byte_count += bytes_read as u64;
        if let Some(limit) = options.max_bytes
            && byte_count > limit
        {
            return Err(too_large_error(
                side,
                "max_bytes",
                limit.to_string(),
                byte_count.to_string(),
            ));
        }

        if line.trim().is_empty() {
            continue;
        }

        if let Some(limit) = options.max_rows
            && records.len() >= limit
        {
            return Err(too_large_error(
                side,
                "max_rows",
                limit.to_string(),
                (records.len() + 1).to_string(),
            ));
        }

        let row_index = records.len() + 1;
        let value: Value = serde_json::from_str(&line).map_err(|error| {
            parse_error(
                path,
                side,
                format!("Invalid JSON on line {row_index}: {error}"),
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            input_contract_error(
                format!(
                    "{} tape JSONL line {row_index} is not an object",
                    side_name(side)
                ),
                json!({
                    "side": side_name(side),
                    "path": path.display().to_string(),
                    "row_index": row_index
                }),
            )
        })?;

        let headers = object.keys().cloned().collect::<Vec<_>>();
        validate_required_columns(path, side, required_columns, &headers)?;

        let attributes = object
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();

        push_record(
            side,
            row_index,
            attributes,
            id_columns,
            &mut seen_ids,
            &mut records,
        )?;
    }

    Ok(LoadedTape {
        side,
        path: path.display().to_string(),
        format: InputFormat::Jsonl,
        delimiter: None,
        records,
    })
}

fn push_record(
    side: TapeSide,
    row_index: usize,
    attributes: BTreeMap<String, Value>,
    id_columns: &[String],
    seen_ids: &mut BTreeSet<String>,
    records: &mut Vec<ResolveRecord>,
) -> ResolveResult<()> {
    let composite_id = build_composite_id(side, row_index, &attributes, id_columns)?;
    if !seen_ids.insert(composite_id.clone()) {
        return Err(input_contract_error(
            format!(
                "{} tape has duplicate composite ID '{}'",
                side_name(side),
                composite_id
            ),
            json!({
                "side": side_name(side),
                "row_index": row_index,
                "composite_id": composite_id
            }),
        ));
    }

    records.push(ResolveRecord {
        side,
        composite_id,
        row_index,
        attributes,
    });
    Ok(())
}

pub fn build_composite_id(
    side: TapeSide,
    row_index: usize,
    attributes: &BTreeMap<String, Value>,
    id_columns: &[String],
) -> ResolveResult<String> {
    let mut parts = Vec::with_capacity(id_columns.len());

    for column in id_columns {
        let value = attributes.get(column).ok_or_else(|| {
            input_contract_error(
                format!(
                    "{} tape is missing identity column '{}'",
                    side_name(side),
                    column
                ),
                json!({
                    "side": side_name(side),
                    "row_index": row_index,
                    "column": column
                }),
            )
        })?;
        let rendered = scalar_to_string(value).ok_or_else(|| {
            input_contract_error(
                format!(
                    "{} tape identity column '{}' must be scalar",
                    side_name(side),
                    column
                ),
                json!({
                    "side": side_name(side),
                    "row_index": row_index,
                    "column": column,
                    "value": value
                }),
            )
        })?;
        let trimmed = rendered.trim();
        if trimmed.is_empty() {
            return Err(input_contract_error(
                format!(
                    "{} tape identity column '{}' is empty at row {}",
                    side_name(side),
                    column,
                    row_index
                ),
                json!({
                    "side": side_name(side),
                    "row_index": row_index,
                    "column": column
                }),
            ));
        }
        if trimmed.contains(COMPOSITE_ID_SEPARATOR) {
            return Err(input_contract_error(
                format!(
                    "{} tape identity value for column '{}' contains reserved separator '{}'",
                    side_name(side),
                    column,
                    COMPOSITE_ID_SEPARATOR
                ),
                json!({
                    "side": side_name(side),
                    "row_index": row_index,
                    "column": column,
                    "separator": COMPOSITE_ID_SEPARATOR,
                    "value": trimmed
                }),
            ));
        }
        parts.push(trimmed.to_string());
    }

    Ok(parts.join(COMPOSITE_ID_SEPARATOR))
}

fn validate_required_columns(
    path: &Path,
    side: TapeSide,
    required_columns: &BTreeSet<String>,
    available_columns: &[String],
) -> ResolveResult<()> {
    let available = available_columns.iter().cloned().collect::<BTreeSet<_>>();
    for column in required_columns {
        if !available.contains(column) {
            return Err(ResolveError::with_detail(
                ResolveErrorCode::InputContract,
                format!(
                    "Column '{}' not found in {} tape '{}'",
                    column,
                    side_name(side),
                    path.display()
                ),
                json!({
                    "side": side_name(side),
                    "path": path.display().to_string(),
                    "column": column,
                    "available_columns": available_columns
                }),
            ));
        }
    }
    Ok(())
}

fn id_columns_for_side(strategy: &ResolveStrategy, side: TapeSide) -> &[String] {
    match side {
        TapeSide::Reference => &strategy.identity.reference.id_columns,
        TapeSide::Target => &strategy.identity.target.id_columns,
    }
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn enforce_max_bytes(path: &Path, side: TapeSide, max_bytes: Option<u64>) -> ResolveResult<()> {
    if let Some(limit) = max_bytes {
        let bytes = std::fs::metadata(path)
            .map_err(|error| io_error(path, side, error))?
            .len();
        if bytes > limit {
            return Err(too_large_error(
                side,
                "max_bytes",
                limit.to_string(),
                bytes.to_string(),
            ));
        }
    }
    Ok(())
}

fn input_error(path: &Path, side: TapeSide, error: input::InputError) -> ResolveError {
    match error {
        input::InputError::Io(message) => ResolveError::with_detail(
            ResolveErrorCode::Io,
            message,
            json!({
                "side": side_name(side),
                "path": path.display().to_string()
            }),
        ),
        input::InputError::Parse(message) | input::InputError::CsvParse(message) => {
            parse_error(path, side, message)
        }
        input::InputError::Encoding(message) => parse_error(path, side, message),
        input::InputError::EmptyInput => ResolveError::with_detail(
            ResolveErrorCode::EmptyTape,
            format!(
                "{} tape '{}' contains no processable records",
                side_name(side),
                path.display()
            ),
            json!({
                "side": side_name(side),
                "path": path.display().to_string()
            }),
        ),
        input::InputError::ColumnNotFound { column, available } => ResolveError::with_detail(
            ResolveErrorCode::InputContract,
            format!("Column '{}' not found in {} tape", column, side_name(side)),
            json!({
                "side": side_name(side),
                "path": path.display().to_string(),
                "column": column,
                "available_columns": available
            }),
        ),
        input::InputError::TooLarge {
            limit_type,
            limit,
            actual,
        } => too_large_error(side, &limit_type, limit, actual),
    }
}

fn io_error(path: &Path, side: TapeSide, error: std::io::Error) -> ResolveError {
    ResolveError::with_detail(
        ResolveErrorCode::Io,
        format!(
            "Unable to read {} tape '{}': {}",
            side_name(side),
            path.display(),
            error
        ),
        json!({
            "side": side_name(side),
            "path": path.display().to_string(),
            "error": error.to_string()
        }),
    )
}

fn parse_error(path: &Path, side: TapeSide, message: impl Into<String>) -> ResolveError {
    ResolveError::with_detail(
        ResolveErrorCode::Parse,
        message,
        json!({
            "side": side_name(side),
            "path": path.display().to_string()
        }),
    )
}

fn input_contract_error(message: impl Into<String>, detail: serde_json::Value) -> ResolveError {
    ResolveError::with_detail(ResolveErrorCode::InputContract, message, detail)
}

fn too_large_error(
    side: TapeSide,
    limit_type: &str,
    limit: String,
    actual: String,
) -> ResolveError {
    ResolveError::with_detail(
        ResolveErrorCode::TooLarge,
        format!(
            "{} tape exceeds --{} limit ({} > {})",
            side_name(side),
            limit_type,
            actual,
            limit
        ),
        json!({
            "side": side_name(side),
            "limit_type": limit_type,
            "limit": limit,
            "actual": actual
        }),
    )
}

fn side_name(side: TapeSide) -> &'static str {
    match side {
        TapeSide::Reference => "reference",
        TapeSide::Target => "target",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::strategy::parse_strategy_bytes;
    use serde_json::json;
    use std::{fs, path::Path};
    use tempfile::NamedTempFile;

    fn fixture(relative: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/resolve")
            .join(relative)
    }

    fn strategy() -> ResolveStrategy {
        parse_strategy_bytes(
            &fs::read(fixture("strategies/cmbs_loans.valid.yaml")).expect("strategy fixture"),
        )
        .expect("strategy parses")
    }

    fn options() -> TapeLoadOptions {
        TapeLoadOptions {
            max_rows: None,
            max_bytes: None,
        }
    }

    #[test]
    fn loads_csv_and_jsonl_tapes_with_composite_ids() {
        let strategy = strategy();
        let reference = load_tape(
            &fixture("tapes/reference_loans.csv"),
            TapeSide::Reference,
            &strategy,
            options(),
        )
        .unwrap();
        assert_eq!(reference.records.len(), 10);
        assert_eq!(reference.records[0].composite_id, "223232");
        assert_eq!(reference.delimiter, Some(b','));

        let target = load_tape(
            &fixture("tapes/target_loans.jsonl"),
            TapeSide::Target,
            &strategy,
            options(),
        )
        .unwrap();
        assert_eq!(target.records.len(), 12);
        assert_eq!(target.records[0].composite_id, "WFCM2019-C50|1");
        assert_eq!(target.format, InputFormat::Jsonl);
    }

    #[test]
    fn sorted_records_are_deterministic_without_mutating_file_order() {
        let strategy = strategy();
        let target = load_tape(
            &fixture("tapes/target_loans.csv"),
            TapeSide::Target,
            &strategy,
            options(),
        )
        .unwrap();
        assert_eq!(target.records[0].composite_id, "WFCM2019-C50|1");

        let sorted = target.records_sorted_by_id();
        let sorted_ids = sorted
            .iter()
            .map(|record| record.composite_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(sorted_ids[0], "WFCM2019-C50|1");
        assert_eq!(sorted_ids[1], "WFCM2019-C50|2");
    }

    #[test]
    fn refuses_missing_columns_and_empty_tapes() {
        let strategy = strategy();
        let missing = load_tape(
            &fixture("tapes/missing_column_target.csv"),
            TapeSide::Target,
            &strategy,
            options(),
        )
        .unwrap_err();
        assert_eq!(missing.code, ResolveErrorCode::InputContract);
        assert_eq!(missing.detail.unwrap()["column"], "balance");

        let empty = load_tape(
            &fixture("tapes/empty_target.csv"),
            TapeSide::Target,
            &strategy,
            options(),
        )
        .unwrap_err();
        assert_eq!(empty.code, ResolveErrorCode::EmptyTape);
    }

    #[test]
    fn refuses_duplicate_and_invalid_composite_ids() {
        let duplicate_file = NamedTempFile::with_suffix(".csv").unwrap();
        fs::write(
            duplicate_file.path(),
            "deal,loan_number,address,balance,coupon,servicer_name,maturity,orig_date\nD,1,A,1,0.1,Wells Fargo,2030-01-01,2020-01-01\nD,1,B,2,0.1,Wells Fargo,2030-01-01,2020-01-01\n",
        )
        .unwrap();

        let minimal = parse_strategy_bytes(
            br#"
strategy_id: duplicate-test.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [deal, loan_number]
  target:
    id_columns: [deal, loan_number]
assertions:
  - field_ref: address
    field_tgt: address
    op: exact
    weight: 1.0
match_threshold: 1.0
ambiguity_gap: 0.1
"#,
        )
        .unwrap();

        let duplicate =
            load_tape(duplicate_file.path(), TapeSide::Target, &minimal, options()).unwrap_err();
        assert_eq!(duplicate.code, ResolveErrorCode::InputContract);
        assert!(duplicate.message.contains("duplicate"));

        let attrs = BTreeMap::from([
            ("deal".to_string(), json!("D|BAD")),
            ("loan_number".to_string(), json!("1")),
        ]);
        let bad_id = build_composite_id(
            TapeSide::Target,
            1,
            &attrs,
            &["deal".to_string(), "loan_number".to_string()],
        )
        .unwrap_err();
        assert_eq!(bad_id.code, ResolveErrorCode::InputContract);
    }

    #[test]
    fn enforces_max_rows_and_max_bytes() {
        let strategy = strategy();
        let row_error = load_tape(
            &fixture("tapes/target_loans.csv"),
            TapeSide::Target,
            &strategy,
            TapeLoadOptions {
                max_rows: Some(1),
                max_bytes: None,
            },
        )
        .unwrap_err();
        assert_eq!(row_error.code, ResolveErrorCode::TooLarge);

        let byte_error = load_tape(
            &fixture("tapes/target_loans.csv"),
            TapeSide::Target,
            &strategy,
            TapeLoadOptions {
                max_rows: None,
                max_bytes: Some(1),
            },
        )
        .unwrap_err();
        assert_eq!(byte_error.code, ResolveErrorCode::TooLarge);
    }

    #[test]
    fn load_tapes_returns_reference_and_target() {
        let strategy = strategy();
        let loaded = load_tapes(
            &fixture("tapes/reference_loans.csv"),
            &fixture("tapes/target_loans.csv"),
            &strategy,
            options(),
        )
        .unwrap();
        assert_eq!(loaded.reference.records.len(), 10);
        assert_eq!(loaded.target.records.len(), 12);
        assert_eq!(loaded.target.summary().record_count, 12);
    }
}
