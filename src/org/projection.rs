//! Projection of normalized rows into `canon org` observations.

use super::types::{
    OrgError, OrgErrorCode, OrgResult, OrgSideField, OrgStrategy, ProjectedAnchor,
    ProjectedObservation, ProjectedSurface,
};
use csv::{ReaderBuilder, StringRecord};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

pub fn project_input(
    input_path: &Path,
    strategy: &OrgStrategy,
    max_bytes: Option<u64>,
    max_rows: Option<usize>,
) -> OrgResult<Vec<ProjectedObservation>> {
    let format = detect_format(input_path)?;

    if input_path != Path::new("-")
        && let Some(limit) = max_bytes
    {
        let file_size = std::fs::metadata(input_path)
            .map_err(|error| {
                input_contract_error(
                    "Failed to read input metadata",
                    json!({
                        "input": input_path.display().to_string(),
                        "error": error.to_string(),
                    }),
                )
            })?
            .len();

        if file_size > limit {
            return Err(limit_error(
                "max_bytes",
                limit.to_string(),
                file_size.to_string(),
            ));
        }
    }

    let observations = match format {
        ProjectionInputFormat::Csv => project_csv(input_path, strategy, max_rows)?,
        ProjectionInputFormat::Jsonl => project_jsonl(input_path, strategy, max_bytes, max_rows)?,
    };

    if observations.is_empty() {
        return Err(input_contract_error(
            "Input contains no processable org rows",
            json!({
                "input": input_path.display().to_string(),
            }),
        ));
    }

    Ok(observations)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionInputFormat {
    Csv,
    Jsonl,
}

#[derive(Debug, Clone)]
struct RawRow {
    row_number: usize,
    values: BTreeMap<String, Value>,
}

impl RawRow {
    fn get(&self, field: &str) -> Option<&Value> {
        self.values.get(field)
    }

    fn required_string(&self, field: &str) -> OrgResult<String> {
        let value = self.get(field).ok_or_else(|| {
            input_contract_error(
                format!("Missing required field '{}'", field),
                json!({
                    "row_number": self.row_number,
                    "field": field,
                }),
            )
        })?;

        scalar_string(value, field, self.row_number)?.ok_or_else(|| {
            input_contract_error(
                format!("Required field '{}' must be non-empty", field),
                json!({
                    "row_number": self.row_number,
                    "field": field,
                }),
            )
        })
    }
}

fn detect_format(input_path: &Path) -> OrgResult<ProjectionInputFormat> {
    if input_path == Path::new("-") {
        return Ok(ProjectionInputFormat::Jsonl);
    }

    let extension = input_path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| {
            input_contract_error(
                "Input path must use a supported extension",
                json!({
                    "input": input_path.display().to_string(),
                }),
            )
        })?;

    match extension.to_ascii_lowercase().as_str() {
        "csv" | "tsv" => Ok(ProjectionInputFormat::Csv),
        "jsonl" | "ndjson" => Ok(ProjectionInputFormat::Jsonl),
        _ => Err(input_contract_error(
            "Unsupported org input format",
            json!({
                "input": input_path.display().to_string(),
                "extension": extension,
            }),
        )),
    }
}

fn project_csv(
    input_path: &Path,
    strategy: &OrgStrategy,
    max_rows: Option<usize>,
) -> OrgResult<Vec<ProjectedObservation>> {
    let file = File::open(input_path).map_err(|error| {
        input_contract_error(
            "Failed to open org input",
            json!({
                "input": input_path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    let delimiter = detect_csv_delimiter(&file)?;
    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(file);

    let headers = reader.headers().map_err(|error| {
        input_contract_error(
            "Failed to read CSV headers",
            json!({
                "input": input_path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;
    let header_names = headers
        .iter()
        .map(|field| field.to_string())
        .collect::<Vec<_>>();
    validate_csv_headers(&header_names, strategy)?;

    let mut observations = Vec::new();
    let mut row_count = 0usize;

    for result in reader.records() {
        let record = result.map_err(|error| {
            input_contract_error(
                "Failed to parse CSV org row",
                json!({
                    "input": input_path.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?;

        if is_blank_record(&record) {
            continue;
        }

        if let Some(limit) = max_rows
            && row_count >= limit
        {
            return Err(limit_error(
                "max_rows",
                limit.to_string(),
                row_count.to_string(),
            ));
        }

        row_count += 1;
        let row = csv_row_to_raw(row_count, &header_names, &record);
        observations.push(project_row(&row, strategy)?);
    }

    Ok(observations)
}

fn project_jsonl(
    input_path: &Path,
    strategy: &OrgStrategy,
    max_bytes: Option<u64>,
    max_rows: Option<usize>,
) -> OrgResult<Vec<ProjectedObservation>> {
    if input_path == Path::new("-") {
        let stdin = io::stdin();
        project_jsonl_reader(stdin.lock(), strategy, max_bytes, max_rows, true)
    } else {
        let file = File::open(input_path).map_err(|error| {
            input_contract_error(
                "Failed to open org input",
                json!({
                    "input": input_path.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?;
        project_jsonl_reader(BufReader::new(file), strategy, max_bytes, max_rows, false)
    }
}

fn project_jsonl_reader<R: BufRead>(
    mut reader: R,
    strategy: &OrgStrategy,
    max_bytes: Option<u64>,
    max_rows: Option<usize>,
    track_bytes: bool,
) -> OrgResult<Vec<ProjectedObservation>> {
    let mut observations = Vec::new();
    let mut row_count = 0usize;
    let mut byte_count = 0u64;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).map_err(|error| {
            input_contract_error(
                "Failed to read JSONL org input",
                json!({
                    "error": error.to_string(),
                }),
            )
        })?;
        if bytes_read == 0 {
            break;
        }

        if track_bytes {
            byte_count += bytes_read as u64;
            if let Some(limit) = max_bytes
                && byte_count > limit
            {
                return Err(limit_error(
                    "max_bytes",
                    limit.to_string(),
                    byte_count.to_string(),
                ));
            }
        }

        if line.trim().is_empty() {
            continue;
        }

        if let Some(limit) = max_rows
            && row_count >= limit
        {
            return Err(limit_error(
                "max_rows",
                limit.to_string(),
                row_count.to_string(),
            ));
        }

        row_count += 1;

        let value = serde_json::from_str::<Value>(&line).map_err(|error| {
            input_contract_error(
                "Invalid JSONL org row",
                json!({
                    "row_number": row_count,
                    "error": error.to_string(),
                }),
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            input_contract_error(
                "JSONL org rows must be JSON objects",
                json!({
                    "row_number": row_count,
                }),
            )
        })?;

        let row = json_object_to_raw(row_count, object);
        observations.push(project_row(&row, strategy)?);
    }

    Ok(observations)
}

fn validate_csv_headers(headers: &[String], strategy: &OrgStrategy) -> OrgResult<()> {
    let header_set = headers.iter().map(String::as_str).collect::<BTreeSet<_>>();

    for field in ["source_row_id", "doc_id", "as_of_date"] {
        if !header_set.contains(field) {
            return Err(input_contract_error(
                format!("CSV input is missing required field '{}'", field),
                json!({
                    "field": field,
                    "available_fields": headers,
                }),
            ));
        }
    }

    if !strategy
        .observations
        .name_fields
        .iter()
        .any(|field| header_set.contains(field.as_str()))
    {
        return Err(input_contract_error(
            "CSV input does not include any configured name field",
            json!({
                "name_fields": strategy.observations.name_fields,
                "available_fields": headers,
            }),
        ));
    }

    for side_field in &strategy.observations.required_side_fields {
        let field_name = side_field_name(*side_field);
        if !header_set.contains(field_name) {
            return Err(input_contract_error(
                format!("Missing required side field '{}'", field_name),
                json!({
                    "field": field_name,
                    "available_fields": headers,
                }),
            ));
        }
    }

    Ok(())
}

fn csv_row_to_raw(row_number: usize, headers: &[String], record: &StringRecord) -> RawRow {
    let mut values = BTreeMap::new();
    for (index, header) in headers.iter().enumerate() {
        let field = record.get(index).unwrap_or_default().to_string();
        values.insert(header.clone(), Value::String(field));
    }

    RawRow { row_number, values }
}

fn json_object_to_raw(row_number: usize, object: &Map<String, Value>) -> RawRow {
    RawRow {
        row_number,
        values: object
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }
}

fn project_row(row: &RawRow, strategy: &OrgStrategy) -> OrgResult<ProjectedObservation> {
    let source_row_id = row.required_string("source_row_id")?;
    let doc_id = row.required_string("doc_id")?;
    let as_of_date = row.required_string("as_of_date")?;

    let primary_surface = primary_surface(row, strategy)?;
    let alias_surfaces = side_surfaces(
        row,
        "alias_surfaces_json",
        strategy
            .observations
            .required_side_fields
            .contains(&OrgSideField::AliasSurfacesJson),
    )?;
    let mention_surfaces = side_surfaces(
        row,
        "mention_surfaces_json",
        strategy
            .observations
            .required_side_fields
            .contains(&OrgSideField::MentionSurfacesJson),
    )?;

    let anchors = projected_anchors(row, strategy)?;
    let context = projected_context(row, strategy)?;
    let provenance = projected_provenance(
        &primary_surface,
        &alias_surfaces,
        &mention_surfaces,
        &anchors,
        &context,
    );

    Ok(ProjectedObservation {
        source_row_id,
        doc_id,
        as_of_date: Some(as_of_date),
        primary_surface,
        alias_surfaces,
        mention_surfaces,
        anchors,
        context,
        provenance,
        ..ProjectedObservation::default()
    })
}

fn primary_surface(row: &RawRow, strategy: &OrgStrategy) -> OrgResult<ProjectedSurface> {
    for field in &strategy.observations.name_fields {
        match row.get(field) {
            None | Some(Value::Null) => continue,
            Some(value) => {
                if let Some(surface) = scalar_string(value, field, row.row_number)? {
                    return Ok(ProjectedSurface {
                        value: surface,
                        field: field.clone(),
                    });
                }
            }
        }
    }

    Err(input_contract_error(
        "No configured primary name field produced a non-empty value",
        json!({
            "row_number": row.row_number,
            "name_fields": strategy.observations.name_fields,
        }),
    ))
}

fn side_surfaces(
    row: &RawRow,
    field_name: &str,
    required: bool,
) -> OrgResult<Vec<ProjectedSurface>> {
    let value = match row.get(field_name) {
        None => {
            if required {
                return Err(input_contract_error(
                    format!("Missing required side field '{}'", field_name),
                    json!({
                        "row_number": row.row_number,
                        "field": field_name,
                    }),
                ));
            }
            return Ok(Vec::new());
        }
        Some(value) => value,
    };

    let decoded = decode_side_field(value, field_name, row.row_number)?;
    Ok(decoded
        .into_iter()
        .map(|surface| ProjectedSurface {
            value: surface,
            field: field_name.to_string(),
        })
        .collect())
}

fn decode_side_field(value: &Value, field_name: &str, row_number: usize) -> OrgResult<Vec<String>> {
    let array = match value {
        Value::Array(items) => items.clone(),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(input_contract_error(
                    format!(
                        "Side field '{}' must be a JSON array of strings",
                        field_name
                    ),
                    json!({
                        "row_number": row_number,
                        "field": field_name,
                    }),
                ));
            }

            serde_json::from_str::<Value>(trimmed)
                .map_err(|error| {
                    input_contract_error(
                        format!("Side field '{}' must be valid JSON", field_name),
                        json!({
                            "row_number": row_number,
                            "field": field_name,
                            "error": error.to_string(),
                        }),
                    )
                })?
                .as_array()
                .cloned()
                .ok_or_else(|| {
                    input_contract_error(
                        format!("Side field '{}' must decode to a JSON array", field_name),
                        json!({
                            "row_number": row_number,
                            "field": field_name,
                        }),
                    )
                })?
        }
        _ => {
            return Err(input_contract_error(
                format!(
                    "Side field '{}' must be a JSON array of strings",
                    field_name
                ),
                json!({
                    "row_number": row_number,
                    "field": field_name,
                }),
            ));
        }
    };

    let mut surfaces = Vec::with_capacity(array.len());
    for item in array {
        let text = item.as_str().ok_or_else(|| {
            input_contract_error(
                format!("Side field '{}' may contain only strings", field_name),
                json!({
                    "row_number": row_number,
                    "field": field_name,
                }),
            )
        })?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(input_contract_error(
                format!("Side field '{}' may not contain empty strings", field_name),
                json!({
                    "row_number": row_number,
                    "field": field_name,
                }),
            ));
        }
        surfaces.push(trimmed.to_string());
    }

    Ok(surfaces)
}

fn projected_anchors(row: &RawRow, strategy: &OrgStrategy) -> OrgResult<Vec<ProjectedAnchor>> {
    let mut anchors = Vec::new();

    for (namespace, field) in &strategy.observations.anchor_fields {
        let Some(value) = row.get(field) else {
            continue;
        };
        let Some(anchor_value) = scalar_string(value, field, row.row_number)? else {
            continue;
        };

        anchors.push(ProjectedAnchor {
            namespace: namespace.clone(),
            value: anchor_value,
            field: field.clone(),
        });
    }

    Ok(anchors)
}

fn projected_context(row: &RawRow, strategy: &OrgStrategy) -> OrgResult<BTreeMap<String, Value>> {
    let mut context = BTreeMap::new();

    for field in &strategy.observations.context_fields {
        let Some(value) = row.get(field) else {
            continue;
        };

        if let Some(normalized) = normalize_context_value(value) {
            context.insert(field.clone(), normalized);
        }
    }

    Ok(context)
}

fn normalize_context_value(value: &Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(Value::String(trimmed.to_string()))
            }
        }
        other => Some(other.clone()),
    }
}

fn projected_provenance(
    primary_surface: &ProjectedSurface,
    alias_surfaces: &[ProjectedSurface],
    mention_surfaces: &[ProjectedSurface],
    anchors: &[ProjectedAnchor],
    context: &BTreeMap<String, Value>,
) -> BTreeMap<String, Vec<String>> {
    let mut provenance = BTreeMap::new();
    provenance.insert(
        "primary_surface".to_string(),
        vec![primary_surface.field.clone()],
    );

    if !alias_surfaces.is_empty() {
        provenance.insert(
            "alias_surfaces".to_string(),
            vec![alias_surfaces[0].field.clone()],
        );
    }

    if !mention_surfaces.is_empty() {
        provenance.insert(
            "mention_surfaces".to_string(),
            vec![mention_surfaces[0].field.clone()],
        );
    }

    for anchor in anchors {
        provenance.insert(
            format!("anchors.{}", anchor.namespace),
            vec![anchor.field.clone()],
        );
    }

    for field in context.keys() {
        provenance.insert(format!("context.{}", field), vec![field.clone()]);
    }

    provenance
}

fn scalar_string(value: &Value, field: &str, row_number: usize) -> OrgResult<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Value::Number(number) => Ok(Some(number.to_string())),
        Value::Bool(boolean) => Ok(Some(boolean.to_string())),
        Value::Array(_) | Value::Object(_) => Err(input_contract_error(
            format!("Field '{}' must be scalar", field),
            json!({
                "row_number": row_number,
                "field": field,
            }),
        )),
    }
}

fn side_field_name(field: OrgSideField) -> &'static str {
    match field {
        OrgSideField::AliasSurfacesJson => "alias_surfaces_json",
        OrgSideField::MentionSurfacesJson => "mention_surfaces_json",
    }
}

fn detect_csv_delimiter(mut file: &File) -> OrgResult<u8> {
    let mut buffer = vec![0; 8192];
    let initial_pos = file.stream_position().map_err(|error| {
        input_contract_error(
            "Failed to inspect CSV org input",
            json!({
                "error": error.to_string(),
            }),
        )
    })?;

    let bytes_read = file.read(&mut buffer).map_err(|error| {
        input_contract_error(
            "Failed to inspect CSV org input",
            json!({
                "error": error.to_string(),
            }),
        )
    })?;

    file.seek(SeekFrom::Start(initial_pos)).map_err(|error| {
        input_contract_error(
            "Failed to reset CSV org input after delimiter detection",
            json!({
                "error": error.to_string(),
            }),
        )
    })?;

    if bytes_read == 0 {
        return Ok(b',');
    }

    let content = String::from_utf8_lossy(&buffer[..bytes_read]);
    let comma_count = content.matches(',').count();
    let tab_count = content.matches('\t').count();
    let pipe_count = content.matches('|').count();
    let semicolon_count = content.matches(';').count();

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
        Ok(b',')
    }
}

fn is_blank_record(record: &StringRecord) -> bool {
    record.iter().all(|field| field.trim().is_empty())
}

fn input_contract_error(message: impl Into<String>, detail: Value) -> OrgError {
    OrgError::with_detail(OrgErrorCode::InputContract, message, detail)
}

fn limit_error(limit_type: &str, limit: String, actual: String) -> OrgError {
    input_contract_error(
        format!("Input exceeds --{} limit", limit_type),
        json!({
            "limit_type": limit_type,
            "limit": limit,
            "actual": actual,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::org::types::{
        NormalizeConfig, StrategyAnchors, StrategyEvidence, StrategyObservations,
        StrategyPromotion, StrategyReconcile, StrategySolver,
    };
    use tempfile::NamedTempFile;

    fn test_strategy(required_side_fields: Vec<OrgSideField>) -> OrgStrategy {
        OrgStrategy {
            id: "bdc_org_graph.v1".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "issuer".to_string(),
            id_prefix: "IC".to_string(),
            observations: StrategyObservations {
                name_fields: vec!["portfolio_company".to_string()],
                required_side_fields,
                context_fields: vec!["industry".to_string(), "par_amount".to_string()],
                anchor_fields: BTreeMap::from([
                    ("cik".to_string(), "cik".to_string()),
                    ("lei".to_string(), "lei".to_string()),
                ]),
            },
            normalize: NormalizeConfig::default(),
            blocking: Vec::new(),
            evidence: StrategyEvidence::default(),
            solver: StrategySolver::default(),
            reconcile: StrategyReconcile::default(),
            anchors: StrategyAnchors::default(),
            promotion: StrategyPromotion::default(),
            content_hash: "blake3:test".to_string(),
            description: String::new(),
        }
    }

    fn write_csv(headers: &[&str], rows: &[Vec<&str>]) -> NamedTempFile {
        let file = NamedTempFile::with_suffix(".csv").expect("temp csv");
        let mut writer = csv::Writer::from_path(file.path()).expect("csv writer");
        writer.write_record(headers).expect("headers");
        for row in rows {
            writer.write_record(row).expect("row");
        }
        writer.flush().expect("flush");
        file
    }

    fn write_jsonl(lines: &[&str]) -> NamedTempFile {
        let file = NamedTempFile::with_suffix(".jsonl").expect("temp jsonl");
        std::fs::write(file.path(), lines.join("\n")).expect("write jsonl");
        file
    }

    #[test]
    fn projects_csv_rows_with_deterministic_order() {
        let strategy = test_strategy(Vec::new());
        let file = write_csv(
            &[
                "source_row_id",
                "doc_id",
                "as_of_date",
                "portfolio_company",
                "alias_surfaces_json",
                "mention_surfaces_json",
                "industry",
                "par_amount",
                "lei",
                "cik",
            ],
            &[
                vec![
                    "row-2",
                    "doc-b",
                    "2025-12-31",
                    "Bravo Corp.",
                    "[\"Bravo Holdings\"]",
                    "[\"Bravo\"]",
                    "Software",
                    "10",
                    "LEI-B",
                    "",
                ],
                vec![
                    "row-1",
                    "doc-a",
                    "2025-11-30",
                    "Alpha Corp.",
                    "[\"Alpha Holdings\",\"Alpha\"]",
                    "[]",
                    "Finance",
                    "15",
                    "LEI-A",
                    "0001",
                ],
            ],
        );

        let observations = project_input(file.path(), &strategy, None, None).expect("csv project");
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0].source_row_id, "row-2");
        assert_eq!(observations[1].source_row_id, "row-1");
        assert_eq!(
            observations[0].alias_surfaces,
            vec![ProjectedSurface {
                value: "Bravo Holdings".to_string(),
                field: "alias_surfaces_json".to_string(),
            }]
        );
        assert_eq!(
            observations[1].anchors,
            vec![
                ProjectedAnchor {
                    namespace: "cik".to_string(),
                    value: "0001".to_string(),
                    field: "cik".to_string(),
                },
                ProjectedAnchor {
                    namespace: "lei".to_string(),
                    value: "LEI-A".to_string(),
                    field: "lei".to_string(),
                },
            ]
        );
        assert_eq!(
            observations[0].context.get("industry"),
            Some(&Value::String("Software".to_string()))
        );
        assert_eq!(
            observations[0].provenance.get("primary_surface"),
            Some(&vec!["portfolio_company".to_string()])
        );
    }

    #[test]
    fn projects_jsonl_rows_with_typed_context_and_array_side_fields() {
        let strategy = test_strategy(Vec::new());
        let file = write_jsonl(&[
            r#"{"source_row_id":"row-1","doc_id":"doc-1","as_of_date":"2025-12-31","portfolio_company":"Acme Corp.","alias_surfaces_json":["ACME Corporation"],"mention_surfaces_json":"[\"Acme\"]","industry":"Software","par_amount":42,"lei":"549300AAA"}"#,
        ]);

        let observations =
            project_input(file.path(), &strategy, None, None).expect("jsonl project");
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].source_row_id, "row-1");
        assert_eq!(
            observations[0].mention_surfaces,
            vec![ProjectedSurface {
                value: "Acme".to_string(),
                field: "mention_surfaces_json".to_string(),
            }]
        );
        assert_eq!(observations[0].context.get("par_amount"), Some(&json!(42)));
    }

    #[test]
    fn refuses_when_required_side_field_is_missing() {
        let strategy = test_strategy(vec![OrgSideField::AliasSurfacesJson]);
        let file = write_csv(
            &["source_row_id", "doc_id", "as_of_date", "portfolio_company"],
            &[vec!["row-1", "doc-1", "2025-12-31", "Acme Corp."]],
        );

        let error = project_input(file.path(), &strategy, None, None).expect_err("missing side");
        assert_eq!(error.code, OrgErrorCode::InputContract);
        assert!(error.message.contains("Missing required side field"));
    }

    #[test]
    fn refuses_on_malformed_side_field() {
        let strategy = test_strategy(Vec::new());
        let file = write_csv(
            &[
                "source_row_id",
                "doc_id",
                "as_of_date",
                "portfolio_company",
                "alias_surfaces_json",
            ],
            &[vec![
                "row-1",
                "doc-1",
                "2025-12-31",
                "Acme Corp.",
                "[1,\"ok\"]",
            ]],
        );

        let error = project_input(file.path(), &strategy, None, None).expect_err("bad side field");
        assert_eq!(error.code, OrgErrorCode::InputContract);
        assert!(error.message.contains("may contain only strings"));
    }

    #[test]
    fn refuses_when_input_exceeds_max_rows() {
        let strategy = test_strategy(Vec::new());
        let file = write_csv(
            &["source_row_id", "doc_id", "as_of_date", "portfolio_company"],
            &[
                vec!["row-1", "doc-1", "2025-12-31", "Acme Corp."],
                vec!["row-2", "doc-2", "2025-12-31", "Bravo Corp."],
            ],
        );

        let error = project_input(file.path(), &strategy, None, Some(1)).expect_err("max rows");
        assert_eq!(error.code, OrgErrorCode::InputContract);
        assert!(error.message.contains("max_rows"));
    }

    #[test]
    fn refuses_when_input_exceeds_max_bytes() {
        let strategy = test_strategy(Vec::new());
        let file = write_jsonl(&[
            r#"{"source_row_id":"row-1","doc_id":"doc-1","as_of_date":"2025-12-31","portfolio_company":"Acme Corp."}"#,
        ]);
        let byte_len = std::fs::metadata(file.path()).expect("metadata").len();

        let error =
            project_input(file.path(), &strategy, Some(byte_len - 1), None).expect_err("max bytes");
        assert_eq!(error.code, OrgErrorCode::InputContract);
        assert!(error.message.contains("max_bytes"));
    }
}
