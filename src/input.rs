use crate::{InputFormat, InputValues, SpecialReason};
use csv::{ReaderBuilder, StringRecord};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Parse input file based on format and return unified InputValues
pub fn parse_input(
    input_path: &Path,
    column: &str,
    max_bytes: Option<u64>,
    max_rows: Option<usize>,
) -> Result<InputValues, InputError> {
    let format = detect_format(input_path)?;

    // Handle file size check for regular files
    if input_path != Path::new("-")
        && let Some(limit) = max_bytes
    {
        let file_size = std::fs::metadata(input_path)
            .map_err(|e| InputError::Io(format!("Cannot read file metadata: {}", e)))?
            .len();
        if file_size > limit {
            return Err(InputError::TooLarge {
                limit_type: "max_bytes".to_string(),
                limit: limit.to_string(),
                actual: file_size.to_string(),
            });
        }
    }

    let input_values = match format {
        InputFormat::Csv => parse_csv(input_path, column, max_bytes, max_rows)?,
        InputFormat::Jsonl => parse_jsonl(input_path, column, max_bytes, max_rows)?,
    };

    // Check for empty input
    if input_values.values.is_empty() && input_values.special.is_empty() {
        return Err(InputError::EmptyInput);
    }

    Ok(input_values)
}

/// Detect input format from file extension
fn detect_format(input_path: &Path) -> Result<InputFormat, InputError> {
    if input_path == Path::new("-") {
        return Ok(InputFormat::Jsonl);
    }

    let extension = input_path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| InputError::Parse("Missing or unrecognized file extension".to_string()))?;

    match extension.to_lowercase().as_str() {
        "csv" | "tsv" => Ok(InputFormat::Csv),
        "jsonl" | "ndjson" => Ok(InputFormat::Jsonl),
        _ => Err(InputError::Parse(format!(
            "Unrecognized file extension: {}",
            extension
        ))),
    }
}

/// Parse CSV input
fn parse_csv(
    input_path: &Path,
    column: &str,
    _max_bytes: Option<u64>, // Already checked for regular files
    max_rows: Option<usize>,
) -> Result<InputValues, InputError> {
    let file =
        File::open(input_path).map_err(|e| InputError::Io(format!("Cannot open file: {}", e)))?;

    let delimiter = detect_csv_delimiter(&file)?;

    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(file);

    // Get headers and find column index
    let headers = reader
        .headers()
        .map_err(|e| InputError::CsvParse(format!("Cannot read CSV headers: {}", e)))?;

    let column_index =
        headers
            .iter()
            .position(|h| h == column)
            .ok_or_else(|| InputError::ColumnNotFound {
                column: column.to_string(),
                available: headers.iter().map(String::from).collect(),
            })?;

    let mut values = HashMap::new();
    let mut special = HashMap::new();
    let mut row_count = 0;

    for result in reader.records() {
        let record = result.map_err(|e| InputError::CsvParse(format!("CSV parse error: {}", e)))?;

        // Check max_rows limit
        if let Some(limit) = max_rows
            && row_count >= limit
        {
            return Err(InputError::TooLarge {
                limit_type: "max_rows".to_string(),
                limit: limit.to_string(),
                actual: row_count.to_string(),
            });
        }

        // Skip blank records
        if is_blank_record(&record) {
            continue;
        }

        row_count += 1;

        let value = record.get(column_index).unwrap_or("");
        let trimmed = value.trim();

        if trimmed.is_empty() {
            *special.entry(SpecialReason::EmptyValue).or_insert(0) += 1;
        } else {
            values.insert(trimmed.to_string(), ());
        }
    }

    Ok(InputValues {
        values,
        special,
        format: InputFormat::Csv,
        delimiter: Some(delimiter),
    })
}

/// Parse JSONL input
fn parse_jsonl(
    input_path: &Path,
    column: &str,
    max_bytes: Option<u64>,
    max_rows: Option<usize>,
) -> Result<InputValues, InputError> {
    let reader: Box<dyn BufRead> = if input_path == Path::new("-") {
        Box::new(io::stdin().lock())
    } else {
        let file = File::open(input_path)
            .map_err(|e| InputError::Io(format!("Cannot open file: {}", e)))?;
        Box::new(BufReader::new(file))
    };

    let mut values = HashMap::new();
    let mut special = HashMap::new();
    let mut row_count = 0;
    let mut byte_count = 0u64;

    for line_result in reader.lines() {
        let line =
            line_result.map_err(|e| InputError::Io(format!("IO error reading line: {}", e)))?;

        // Check max_bytes limit for stdin
        if input_path == Path::new("-")
            && let Some(limit) = max_bytes
        {
            byte_count += line.len() as u64 + 1; // +1 for newline
            if byte_count > limit {
                return Err(InputError::TooLarge {
                    limit_type: "max_bytes".to_string(),
                    limit: limit.to_string(),
                    actual: byte_count.to_string(),
                });
            }
        }

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Check max_rows limit
        if let Some(limit) = max_rows
            && row_count >= limit
        {
            return Err(InputError::TooLarge {
                limit_type: "max_rows".to_string(),
                limit: limit.to_string(),
                actual: row_count.to_string(),
            });
        }

        row_count += 1;

        let json_value: Value = serde_json::from_str(&line)
            .map_err(|e| InputError::Parse(format!("Invalid JSON on line {}: {}", row_count, e)))?;

        let obj = json_value
            .as_object()
            .ok_or_else(|| InputError::Parse(format!("Line {} is not a JSON object", row_count)))?;

        match obj.get(column) {
            None => {
                *special.entry(SpecialReason::MissingField).or_insert(0) += 1;
            }
            Some(Value::Null) => {
                *special.entry(SpecialReason::NullValue).or_insert(0) += 1;
            }
            Some(Value::Object(_)) | Some(Value::Array(_)) => {
                *special.entry(SpecialReason::NonScalarValue).or_insert(0) += 1;
            }
            Some(value) => {
                let string_value = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => unreachable!(), // Already handled null, object, array above
                };

                let trimmed = string_value.trim();
                if trimmed.is_empty() {
                    *special.entry(SpecialReason::EmptyValue).or_insert(0) += 1;
                } else {
                    values.insert(trimmed.to_string(), ());
                }
            }
        }
    }

    Ok(InputValues {
        values,
        special,
        format: InputFormat::Jsonl,
        delimiter: None,
    })
}

/// Detect CSV delimiter by analyzing first 8KB
fn detect_csv_delimiter(mut file: &File) -> Result<u8, InputError> {
    let mut buffer = vec![0; 8192]; // 8KB buffer
    let initial_pos = file
        .stream_position()
        .map_err(|e| InputError::Io(format!("Cannot get file position: {}", e)))?;

    let bytes_read = file
        .read(&mut buffer)
        .map_err(|e| InputError::Io(format!("Cannot read file for delimiter detection: {}", e)))?;

    // Reset file position
    file.seek(SeekFrom::Start(initial_pos))
        .map_err(|e| InputError::Io(format!("Cannot reset file position: {}", e)))?;

    if bytes_read == 0 {
        return Ok(b','); // Default to comma
    }

    let content = String::from_utf8_lossy(&buffer[..bytes_read]);

    // Count delimiter candidates
    let comma_count = content.matches(',').count();
    let tab_count = content.matches('\t').count();
    let pipe_count = content.matches('|').count();
    let semicolon_count = content.matches(';').count();

    // Return most frequent delimiter
    let max_count = comma_count
        .max(tab_count)
        .max(pipe_count)
        .max(semicolon_count);

    if tab_count == max_count {
        Ok(b'\t')
    } else if pipe_count == max_count {
        Ok(b'|')
    } else if semicolon_count == max_count {
        Ok(b';')
    } else {
        Ok(b',') // Default to comma
    }
}

/// Check if a CSV record is blank (all fields empty after trim)
fn is_blank_record(record: &StringRecord) -> bool {
    record.iter().all(|field| field.trim().is_empty())
}

/// Input parsing errors
#[derive(Debug)]
pub enum InputError {
    Io(String),
    Parse(String),
    CsvParse(String),
    Encoding(String),
    EmptyInput,
    ColumnNotFound {
        column: String,
        available: Vec<String>,
    },
    TooLarge {
        limit_type: String,
        limit: String,
        actual: String,
    },
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputError::Io(msg) => write!(f, "I/O error: {}", msg),
            InputError::Parse(msg) => write!(f, "Parse error: {}", msg),
            InputError::CsvParse(msg) => write!(f, "CSV parse error: {}", msg),
            InputError::Encoding(msg) => write!(f, "Encoding error: {}", msg),
            InputError::EmptyInput => write!(f, "Empty input: no processable data"),
            InputError::ColumnNotFound { column, available } => {
                write!(
                    f,
                    "Column '{}' not found. Available columns: {:?}",
                    column, available
                )
            }
            InputError::TooLarge {
                limit_type,
                limit,
                actual,
            } => {
                write!(
                    f,
                    "Input exceeds --{} limit ({} > {})",
                    limit_type, actual, limit
                )
            }
        }
    }
}

impl std::error::Error for InputError {}

impl InputError {
    pub fn to_refusal_code(&self) -> crate::RefusalCode {
        match self {
            InputError::Io(_) => crate::RefusalCode::EIo,
            InputError::Parse(_) => crate::RefusalCode::EParse,
            InputError::CsvParse(_) => crate::RefusalCode::ECsvParse,
            InputError::Encoding(_) => crate::RefusalCode::EEncoding,
            InputError::EmptyInput => crate::RefusalCode::EEmptyInput,
            InputError::ColumnNotFound { .. } => crate::RefusalCode::EColumnNotFound,
            InputError::TooLarge { .. } => crate::RefusalCode::ETooLarge,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    fn create_test_csv(content: &str) -> NamedTempFile {
        let file = NamedTempFile::with_suffix(".csv").unwrap();
        fs::write(&file, content).unwrap();
        file
    }

    fn create_test_jsonl(content: &str) -> NamedTempFile {
        let file = NamedTempFile::with_suffix(".jsonl").unwrap();
        fs::write(&file, content).unwrap();
        file
    }

    #[test]
    fn test_detect_format() {
        assert!(matches!(
            detect_format(Path::new("test.csv")).unwrap(),
            InputFormat::Csv
        ));
        assert!(matches!(
            detect_format(Path::new("test.tsv")).unwrap(),
            InputFormat::Csv
        ));
        assert!(matches!(
            detect_format(Path::new("test.jsonl")).unwrap(),
            InputFormat::Jsonl
        ));
        assert!(matches!(
            detect_format(Path::new("test.ndjson")).unwrap(),
            InputFormat::Jsonl
        ));
        assert!(matches!(
            detect_format(Path::new("-")).unwrap(),
            InputFormat::Jsonl
        ));

        assert!(detect_format(Path::new("test.txt")).is_err());
        assert!(detect_format(Path::new("test")).is_err());
    }

    #[test]
    fn test_parse_csv_basic() {
        let csv_content = "id,name,value\nA001,Alice,100\nB002,Bob,200\n";
        let file = create_test_csv(csv_content);

        let result = parse_input(file.path(), "id", None, None).unwrap();

        assert_eq!(result.values.len(), 2);
        assert!(result.values.contains_key("A001"));
        assert!(result.values.contains_key("B002"));
        assert!(result.special.is_empty());
        assert!(matches!(result.format, InputFormat::Csv));
        assert_eq!(result.delimiter, Some(b','));
    }

    #[test]
    fn test_parse_csv_empty_values() {
        let csv_content = "id,name\nA001,Alice\n,Bob\nC003,\n";
        let file = create_test_csv(csv_content);

        let result = parse_input(file.path(), "id", None, None).unwrap();

        assert_eq!(result.values.len(), 2);
        assert!(result.values.contains_key("A001"));
        assert!(result.values.contains_key("C003"));
        assert_eq!(result.special[&SpecialReason::EmptyValue], 1);
    }

    #[test]
    fn test_parse_csv_column_not_found() {
        let csv_content = "id,name\nA001,Alice\n";
        let file = create_test_csv(csv_content);

        let result = parse_input(file.path(), "nonexistent", None, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            InputError::ColumnNotFound { .. }
        ));
    }

    #[test]
    fn test_parse_jsonl_basic() {
        let jsonl_content = r#"{"id": "A001", "name": "Alice"}
{"id": "B002", "name": "Bob"}"#;
        let file = create_test_jsonl(jsonl_content);

        let result = parse_input(file.path(), "id", None, None).unwrap();

        assert_eq!(result.values.len(), 2);
        assert!(result.values.contains_key("A001"));
        assert!(result.values.contains_key("B002"));
        assert!(result.special.is_empty());
        assert!(matches!(result.format, InputFormat::Jsonl));
        assert_eq!(result.delimiter, None);
    }

    #[test]
    fn test_parse_jsonl_special_cases() {
        let jsonl_content = r#"{"id": "A001"}
{"name": "No ID"}
{"id": null}
{"id": {"nested": "object"}}
{"id": 42}
{"id": true}
{"id": ""}"#;
        let file = create_test_jsonl(jsonl_content);

        let result = parse_input(file.path(), "id", None, None).unwrap();

        assert_eq!(result.values.len(), 3); // A001, 42, true
        assert!(result.values.contains_key("A001"));
        assert!(result.values.contains_key("42"));
        assert!(result.values.contains_key("true"));

        assert_eq!(result.special[&SpecialReason::MissingField], 1);
        assert_eq!(result.special[&SpecialReason::NullValue], 1);
        assert_eq!(result.special[&SpecialReason::NonScalarValue], 1);
        assert_eq!(result.special[&SpecialReason::EmptyValue], 1);
    }

    #[test]
    fn test_parse_max_rows() {
        let csv_content = "id\nA\nB\nC\n";
        let file = create_test_csv(csv_content);

        let result = parse_input(file.path(), "id", None, Some(2));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), InputError::TooLarge { .. }));
    }

    #[test]
    fn test_parse_empty_input() {
        let csv_content = "id\n";
        let file = create_test_csv(csv_content);

        let result = parse_input(file.path(), "id", None, None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), InputError::EmptyInput));
    }

    #[test]
    fn test_delimiter_detection() {
        // Test comma
        let csv_content = "a,b,c\n1,2,3\n";
        let file = create_test_csv(csv_content);
        let result = parse_input(file.path(), "a", None, None).unwrap();
        assert_eq!(result.delimiter, Some(b','));

        // Test tab
        let csv_content = "a\tb\tc\n1\t2\t3\n";
        let file = create_test_csv(csv_content);
        let result = parse_input(file.path(), "a", None, None).unwrap();
        assert_eq!(result.delimiter, Some(b'\t'));
    }
}
