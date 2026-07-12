use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn emit_csv<W: Write>(
    input_path: &Path,
    resolve_map: &HashMap<String, Option<String>>,
    column: &str,
    canon_column: &str,
    delimiter: u8,
    writer: &mut W,
) -> Result<(), CsvOutputError> {
    let input_file = File::open(input_path)
        .map_err(|e| CsvOutputError::Io(format!("Cannot open file: {}", e)))?;

    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(input_file);

    let headers = reader
        .headers()
        .map_err(|e| CsvOutputError::CsvParse(format!("Cannot read CSV headers: {}", e)))?
        .clone();

    if headers.iter().any(|header| header == canon_column) {
        return Err(CsvOutputError::ColumnExists {
            column: canon_column.to_string(),
        });
    }

    let column_index = headers
        .iter()
        .position(|header| header == column)
        .ok_or_else(|| CsvOutputError::ColumnNotFound {
            column: column.to_string(),
            available: headers.iter().map(String::from).collect(),
        })?;

    let mut csv_writer = WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(writer);

    let mut output_headers = headers.clone();
    output_headers.push_field(canon_column);
    csv_writer
        .write_record(&output_headers)
        .map_err(|e| CsvOutputError::CsvParse(format!("Cannot write CSV headers: {}", e)))?;

    for row in reader.records() {
        let record =
            row.map_err(|e| CsvOutputError::CsvParse(format!("CSV parse error: {}", e)))?;

        let mut output_record = record.clone();
        let canonical_value = if is_blank_record(&record) {
            ""
        } else {
            let lookup_value = ascii_trim(record.get(column_index).unwrap_or(""));
            if lookup_value.is_empty() {
                ""
            } else {
                resolve_map
                    .get(lookup_value)
                    .and_then(|value| value.as_deref())
                    .unwrap_or("")
            }
        };
        output_record.push_field(canonical_value);

        csv_writer
            .write_record(&output_record)
            .map_err(|e| CsvOutputError::CsvParse(format!("Cannot write CSV row: {}", e)))?;
    }

    csv_writer
        .flush()
        .map_err(|e| CsvOutputError::Io(format!("Cannot flush CSV output: {}", e)))?;

    Ok(())
}

fn is_blank_record(record: &StringRecord) -> bool {
    record.iter().all(|field| ascii_trim(field).is_empty())
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

#[derive(Debug)]
pub enum CsvOutputError {
    Io(String),
    CsvParse(String),
    ColumnExists {
        column: String,
    },
    ColumnNotFound {
        column: String,
        available: Vec<String>,
    },
}

impl std::fmt::Display for CsvOutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CsvOutputError::Io(message) => write!(f, "I/O error: {}", message),
            CsvOutputError::CsvParse(message) => write!(f, "CSV parse error: {}", message),
            CsvOutputError::ColumnExists { column } => {
                write!(
                    f,
                    "Canonical column '{}' already exists in CSV header",
                    column
                )
            }
            CsvOutputError::ColumnNotFound { column, available } => write!(
                f,
                "Column '{}' not found. Available columns: {:?}",
                column, available
            ),
        }
    }
}

impl std::error::Error for CsvOutputError {}

impl CsvOutputError {
    pub fn to_refusal_code(&self) -> crate::RefusalCode {
        match self {
            CsvOutputError::Io(_) => crate::RefusalCode::EIo,
            CsvOutputError::CsvParse(_) => crate::RefusalCode::ECsvParse,
            CsvOutputError::ColumnExists { .. } => crate::RefusalCode::EColumnExists,
            CsvOutputError::ColumnNotFound { .. } => crate::RefusalCode::EColumnNotFound,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    fn create_csv_file(contents: &str) -> NamedTempFile {
        let file = NamedTempFile::with_suffix(".csv").unwrap();
        fs::write(file.path(), contents).unwrap();
        file
    }

    #[test]
    fn appends_default_canonical_column() {
        let file = create_csv_file("id,name\nA001,Alice\nB002,Bob\n");
        let mut resolve_map = HashMap::new();
        resolve_map.insert("A001".to_string(), Some("CANON-A".to_string()));
        resolve_map.insert("B002".to_string(), Some("CANON-B".to_string()));

        let mut output = Vec::new();
        emit_csv(
            file.path(),
            &resolve_map,
            "id",
            "id__canon",
            b',',
            &mut output,
        )
        .unwrap();

        let output_text = String::from_utf8(output).unwrap();
        assert_eq!(
            output_text,
            "id,name,id__canon\nA001,Alice,CANON-A\nB002,Bob,CANON-B\n"
        );
    }

    #[test]
    fn appends_custom_canonical_column_name() {
        let file = create_csv_file("id,name\nA001,Alice\n");
        let mut resolve_map = HashMap::new();
        resolve_map.insert("A001".to_string(), Some("CANON-A".to_string()));

        let mut output = Vec::new();
        emit_csv(
            file.path(),
            &resolve_map,
            "id",
            "canonical_id",
            b',',
            &mut output,
        )
        .unwrap();

        let output_text = String::from_utf8(output).unwrap();
        assert_eq!(output_text, "id,name,canonical_id\nA001,Alice,CANON-A\n");
    }

    #[test]
    fn unresolved_rows_get_empty_canonical_column() {
        let file = create_csv_file("id,name\nA001,Alice\nB002,Bob\n");
        let mut resolve_map = HashMap::new();
        resolve_map.insert("A001".to_string(), Some("CANON-A".to_string()));
        resolve_map.insert("B002".to_string(), None);

        let mut output = Vec::new();
        emit_csv(
            file.path(),
            &resolve_map,
            "id",
            "id__canon",
            b',',
            &mut output,
        )
        .unwrap();

        let output_text = String::from_utf8(output).unwrap();
        assert_eq!(
            output_text,
            "id,name,id__canon\nA001,Alice,CANON-A\nB002,Bob,\n"
        );
    }

    #[test]
    fn blank_rows_are_preserved_with_empty_canonical_column() {
        let file = create_csv_file("id,name\nA001,Alice\n,\nB002,Bob\n");
        let mut resolve_map = HashMap::new();
        resolve_map.insert("A001".to_string(), Some("CANON-A".to_string()));
        resolve_map.insert("B002".to_string(), Some("CANON-B".to_string()));

        let mut output = Vec::new();
        emit_csv(
            file.path(),
            &resolve_map,
            "id",
            "id__canon",
            b',',
            &mut output,
        )
        .unwrap();

        let output_text = String::from_utf8(output).unwrap();
        assert_eq!(
            output_text,
            "id,name,id__canon\nA001,Alice,CANON-A\n,,\nB002,Bob,CANON-B\n"
        );
    }

    #[test]
    fn delimiter_is_preserved_for_common_formats() {
        let cases = vec![
            (
                b',',
                "id,name\nA001,Alice\n",
                "id,name,id__canon\nA001,Alice,CANON-A\n",
            ),
            (
                b'\t',
                "id\tname\nA001\tAlice\n",
                "id\tname\tid__canon\nA001\tAlice\tCANON-A\n",
            ),
            (
                b'|',
                "id|name\nA001|Alice\n",
                "id|name|id__canon\nA001|Alice|CANON-A\n",
            ),
        ];

        for (delimiter, input_csv, expected_csv) in cases {
            let file = create_csv_file(input_csv);
            let mut resolve_map = HashMap::new();
            resolve_map.insert("A001".to_string(), Some("CANON-A".to_string()));

            let mut output = Vec::new();
            emit_csv(
                file.path(),
                &resolve_map,
                "id",
                "id__canon",
                delimiter,
                &mut output,
            )
            .unwrap();

            let output_text = String::from_utf8(output).unwrap();
            assert_eq!(output_text, expected_csv);
        }
    }

    #[test]
    fn lookup_uses_ascii_trim_only() {
        let file = create_csv_file("id,name\n A001\t,Alice\nA001\u{00a0},NotAlice\n");
        let mut resolve_map = HashMap::new();
        resolve_map.insert("A001".to_string(), Some("CANON-A".to_string()));

        let mut output = Vec::new();
        emit_csv(
            file.path(),
            &resolve_map,
            "id",
            "id__canon",
            b',',
            &mut output,
        )
        .unwrap();

        let output_text = String::from_utf8(output).unwrap();
        assert_eq!(
            output_text,
            "id,name,id__canon\n A001\t,Alice,CANON-A\nA001\u{00a0},NotAlice,\n"
        );
    }

    #[test]
    fn returns_error_when_canonical_column_already_exists() {
        let file = create_csv_file("id,name,id__canon\nA001,Alice,OLD\n");
        let resolve_map = HashMap::new();
        let mut output = Vec::new();

        let error = emit_csv(
            file.path(),
            &resolve_map,
            "id",
            "id__canon",
            b',',
            &mut output,
        )
        .unwrap_err();

        assert!(matches!(error, CsvOutputError::ColumnExists { .. }));
    }

    #[test]
    fn writes_to_provided_writer() {
        let file = create_csv_file("id,name\nA001,Alice\n");
        let mut resolve_map = HashMap::new();
        resolve_map.insert("A001".to_string(), Some("CANON-A".to_string()));
        let mut buffer = Vec::<u8>::new();

        emit_csv(
            file.path(),
            &resolve_map,
            "id",
            "id__canon",
            b',',
            &mut buffer,
        )
        .unwrap();

        assert!(!buffer.is_empty());
        assert_eq!(
            String::from_utf8(buffer).unwrap(),
            "id,name,id__canon\nA001,Alice,CANON-A\n"
        );
    }
}
