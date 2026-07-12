#![forbid(unsafe_code)]

//! Deterministic N-source entity-link materialization.
//!
//! This module keeps the existing two-tape link path as a covered subset while
//! adding source names, explicit comparison graph validation, pair budgets, and
//! source-aware consistency diagnostics. It does not infer new equivalence
//! evidence; callers provide exact source rows and optional exact anchor fields.

use super::{
    LINK_SIDE_COLUMN, LINK_SOURCE_NAME_COLUMN, LINK_SOURCE_ORDINAL_COLUMN, LINK_SOURCE_ROW_COLUMN,
};
use crate::{Refusal, entity::error::EntityRefusalKind};
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::Path,
};

pub const ENTITY_MULTISOURCE_LINK_VERSION: &str = "canon_entity_multisource_link.v0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMultisourceLinkRequest<'a> {
    pub sources: Vec<EntityNamedSource<'a>>,
    pub comparison_graph: Vec<EntitySourceComparison>,
    pub canonical_source: Option<&'a str>,
    pub default_pair_budget: u64,
    pub output_rows: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityNamedSource<'a> {
    pub name: &'a str,
    pub role: EntitySourceRole,
    pub rows_path: &'a Path,
    pub local_id_column: Option<&'a str>,
    pub anchor_namespace: Option<&'a str>,
    pub anchor_column: Option<&'a str>,
    pub canonical_id_column: Option<&'a str>,
}

impl<'a> EntityNamedSource<'a> {
    pub fn new(name: &'a str, role: EntitySourceRole, rows_path: &'a Path) -> Self {
        Self {
            name,
            role,
            rows_path,
            local_id_column: None,
            anchor_namespace: None,
            anchor_column: None,
            canonical_id_column: None,
        }
    }

    pub fn local_id_column(mut self, column: &'a str) -> Self {
        self.local_id_column = Some(column);
        self
    }

    pub fn anchor(
        mut self,
        namespace: &'a str,
        anchor_column: &'a str,
        canonical_id_column: &'a str,
    ) -> Self {
        self.anchor_namespace = Some(namespace);
        self.anchor_column = Some(anchor_column);
        self.canonical_id_column = Some(canonical_id_column);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntitySourceRole {
    CanonicalReference,
    Reference,
    Target,
    Peer,
}

impl EntitySourceRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalReference => "canonical_reference",
            Self::Reference => "reference",
            Self::Target => "target",
            Self::Peer => "peer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySourceComparison {
    pub left_source: String,
    pub right_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_candidate_rows: Option<u64>,
}

impl EntitySourceComparison {
    pub fn new(left_source: impl Into<String>, right_source: impl Into<String>) -> Self {
        Self {
            left_source: left_source.into(),
            right_source: right_source.into(),
            max_candidate_rows: None,
        }
    }

    pub fn with_budget(
        left_source: impl Into<String>,
        right_source: impl Into<String>,
        max_candidate_rows: u64,
    ) -> Self {
        Self {
            left_source: left_source.into(),
            right_source: right_source.into(),
            max_candidate_rows: Some(max_candidate_rows),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityMultisourceLinkArtifact {
    pub version: String,
    pub mode: EntityMultisourceLinkMode,
    pub source_count: usize,
    pub row_count: u64,
    pub canonical_source: Option<String>,
    pub materialized_rows_path: String,
    pub sources: Vec<EntityMultisourceInput>,
    pub comparison_graph: Vec<EntityComparisonDiagnostic>,
    pub consistency: EntityMultisourceConsistency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityMultisourceLinkMode {
    NamedSources,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityMultisourceInput {
    pub name: String,
    pub role: EntitySourceRole,
    pub rows_path: String,
    pub row_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_id_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityComparisonDiagnostic {
    pub left_source: String,
    pub right_source: String,
    pub left_rows: u64,
    pub right_rows: u64,
    pub candidate_pair_rows: u64,
    pub max_candidate_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityMultisourceConsistency {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchor_conflicts: Vec<EntityAnchorConflict>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abstentions: Vec<EntityMultisourceAbstention>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityAnchorConflict {
    pub anchor_key: String,
    pub canonical_ids: Vec<String>,
    pub source_rows: Vec<EntitySourceRowRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySourceRowRef {
    pub source: String,
    pub row_id: String,
    pub canonical_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityMultisourceAbstention {
    pub reason: EntityMultisourceAbstentionReason,
    pub anchor_key: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityMultisourceAbstentionReason {
    AnchorConflict,
}

pub fn complete_comparison_graph(
    source_names: impl IntoIterator<Item = impl Into<String>>,
) -> Vec<EntitySourceComparison> {
    let names = source_names
        .into_iter()
        .map(Into::into)
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut graph = Vec::new();
    for (left_index, left) in names.iter().enumerate() {
        for right in names.iter().skip(left_index + 1) {
            graph.push(EntitySourceComparison::new(left.clone(), right.clone()));
        }
    }
    graph
}

pub fn materialize_multisource_rows(
    request: EntityMultisourceLinkRequest<'_>,
) -> Result<EntityMultisourceLinkArtifact, Refusal> {
    let normalized = normalize_sources(request.sources)?;
    if normalized.len() < 2 {
        return Err(input_refusal(
            "Entity multisource link requires at least two named sources",
            json!({
                "stage": "link",
                "reason": "too_few_sources",
                "source_count": normalized.len(),
                "writes_performed": false
            }),
        ));
    }
    let canonical_source = request
        .canonical_source
        .map(normalize_source_name)
        .transpose()?;
    if let Some(canonical_source) = &canonical_source
        && !normalized
            .iter()
            .any(|source| &source.name == canonical_source)
    {
        return Err(input_refusal(
            "Entity multisource canonical source is not present in sources",
            json!({
                "stage": "link",
                "reason": "unknown_canonical_source",
                "canonical_source": canonical_source,
                "sources": normalized.iter().map(|source| source.name.clone()).collect::<Vec<_>>(),
                "writes_performed": false
            }),
        ));
    }

    let mut source_rows = Vec::new();
    for source in &normalized {
        source_rows.push(read_source(source)?);
    }

    let graph = normalize_comparison_graph(
        request.comparison_graph,
        request.default_pair_budget,
        &source_rows,
    )?;
    let consistency = detect_anchor_conflicts(&source_rows);
    write_materialized_rows(&source_rows, request.output_rows)?;

    Ok(EntityMultisourceLinkArtifact {
        version: ENTITY_MULTISOURCE_LINK_VERSION.to_string(),
        mode: EntityMultisourceLinkMode::NamedSources,
        source_count: source_rows.len(),
        row_count: source_rows.iter().map(|source| source.row_count()).sum(),
        canonical_source,
        materialized_rows_path: request.output_rows.display().to_string(),
        sources: source_rows.iter().map(SourceRows::artifact_input).collect(),
        comparison_graph: graph,
        consistency,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedSource {
    name: String,
    role: EntitySourceRole,
    rows_path: String,
    local_id_column: Option<String>,
    anchor_namespace: Option<String>,
    anchor_column: Option<String>,
    canonical_id_column: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceRows {
    source: NormalizedSource,
    headers: StringRecord,
    records: Vec<StringRecord>,
}

impl SourceRows {
    fn row_count(&self) -> u64 {
        self.records.len() as u64
    }

    fn artifact_input(&self) -> EntityMultisourceInput {
        EntityMultisourceInput {
            name: self.source.name.clone(),
            role: self.source.role,
            rows_path: self.source.rows_path.clone(),
            row_count: self.row_count(),
            local_id_column: self.source.local_id_column.clone(),
            anchor_namespace: self.source.anchor_namespace.clone(),
        }
    }

    fn header_index(&self) -> BTreeMap<String, usize> {
        self.headers
            .iter()
            .enumerate()
            .map(|(index, header)| (header.to_string(), index))
            .collect()
    }
}

fn normalize_sources(
    sources: Vec<EntityNamedSource<'_>>,
) -> Result<Vec<NormalizedSource>, Refusal> {
    let mut normalized = sources
        .into_iter()
        .map(|source| {
            let anchor_fields = [
                source.anchor_namespace.is_some(),
                source.anchor_column.is_some(),
                source.canonical_id_column.is_some(),
            ];
            if anchor_fields.iter().any(|present| *present)
                && !anchor_fields.iter().all(|present| *present)
            {
                return Err(input_refusal(
                    "Entity multisource anchors require namespace, anchor column, and canonical ID column",
                    json!({
                        "stage": "link",
                        "reason": "incomplete_anchor_config",
                        "source": source.name,
                        "writes_performed": false
                    }),
                ));
            }
            Ok(NormalizedSource {
                name: normalize_source_name(source.name)?,
                role: source.role,
                rows_path: source.rows_path.display().to_string(),
                local_id_column: source
                    .local_id_column
                    .map(normalize_column_name)
                    .transpose()?,
                anchor_namespace: source
                    .anchor_namespace
                    .map(normalize_anchor_namespace)
                    .transpose()?,
                anchor_column: source.anchor_column.map(normalize_column_name).transpose()?,
                canonical_id_column: source
                    .canonical_id_column
                    .map(normalize_column_name)
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    normalized.sort_by(|left, right| left.name.cmp(&right.name));
    for window in normalized.windows(2) {
        if window[0].name == window[1].name {
            return Err(input_refusal(
                "Entity multisource source names must be unique",
                json!({
                    "stage": "link",
                    "reason": "duplicate_source_name",
                    "source": window[0].name,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(normalized)
}

fn normalize_comparison_graph(
    graph: Vec<EntitySourceComparison>,
    default_pair_budget: u64,
    sources: &[SourceRows],
) -> Result<Vec<EntityComparisonDiagnostic>, Refusal> {
    if default_pair_budget == 0 {
        return Err(budget_refusal(
            "Entity multisource default pair budget must be greater than zero",
            json!({
                "stage": "link",
                "reason": "zero_default_pair_budget",
                "writes_performed": false
            }),
        ));
    }
    let source_map = sources
        .iter()
        .map(|source| (source.source.name.clone(), source))
        .collect::<BTreeMap<_, _>>();
    let mut normalized = BTreeMap::<(String, String), u64>::new();
    for edge in graph {
        let left = normalize_source_name(&edge.left_source)?;
        let right = normalize_source_name(&edge.right_source)?;
        if left == right {
            return Err(input_refusal(
                "Entity multisource comparison edges must connect two distinct sources",
                json!({
                    "stage": "link",
                    "reason": "self_comparison_edge",
                    "source": left,
                    "writes_performed": false
                }),
            ));
        }
        if !source_map.contains_key(&left) || !source_map.contains_key(&right) {
            return Err(input_refusal(
                "Entity multisource comparison edge references an unknown source",
                json!({
                    "stage": "link",
                    "reason": "unknown_comparison_source",
                    "left_source": left,
                    "right_source": right,
                    "sources": source_map.keys().cloned().collect::<Vec<_>>(),
                    "writes_performed": false
                }),
            ));
        }
        let (first, second) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };
        let budget = edge.max_candidate_rows.unwrap_or(default_pair_budget);
        if budget == 0 {
            return Err(budget_refusal(
                "Entity multisource pair budget must be greater than zero",
                json!({
                    "stage": "link",
                    "reason": "zero_pair_budget",
                    "left_source": first,
                    "right_source": second,
                    "writes_performed": false
                }),
            ));
        }
        normalized
            .entry((first, second))
            .and_modify(|existing| *existing = (*existing).min(budget))
            .or_insert(budget);
    }
    if normalized.is_empty() {
        return Err(input_refusal(
            "Entity multisource link requires an explicit comparison graph",
            json!({
                "stage": "link",
                "reason": "empty_comparison_graph",
                "writes_performed": false
            }),
        ));
    }

    normalized
        .into_iter()
        .map(|((left, right), max_candidate_rows)| {
            let left_rows = source_map
                .get(&left)
                .expect("validated left source")
                .row_count();
            let right_rows = source_map
                .get(&right)
                .expect("validated right source")
                .row_count();
            let candidate_pair_rows = left_rows.checked_mul(right_rows).ok_or_else(|| {
                budget_refusal(
                    "Entity multisource comparison pair count overflowed",
                    json!({
                        "stage": "link",
                        "reason": "pair_count_overflow",
                        "left_source": left,
                        "right_source": right,
                        "left_rows": left_rows,
                        "right_rows": right_rows,
                        "writes_performed": false
                    }),
                )
            })?;
            if candidate_pair_rows > max_candidate_rows {
                return Err(budget_refusal(
                    "Entity multisource comparison exceeds pair budget",
                    json!({
                        "stage": "link",
                        "reason": "pair_budget_exceeded",
                        "left_source": left,
                        "right_source": right,
                        "left_rows": left_rows,
                        "right_rows": right_rows,
                        "candidate_pair_rows": candidate_pair_rows,
                        "max_candidate_rows": max_candidate_rows,
                        "writes_performed": false
                    }),
                ));
            }
            Ok(EntityComparisonDiagnostic {
                left_source: left,
                right_source: right,
                left_rows,
                right_rows,
                candidate_pair_rows,
                max_candidate_rows,
            })
        })
        .collect()
}

fn read_source(source: &NormalizedSource) -> Result<SourceRows, Refusal> {
    let path = Path::new(&source.rows_path);
    let mut reader = ReaderBuilder::new().from_path(path).map_err(|error| {
        input_refusal(
            "Failed to open entity multisource CSV rows",
            json!({
                "stage": "link",
                "source": source.name,
                "path": source.rows_path,
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let headers = reader.headers().cloned().map_err(|error| {
        input_refusal(
            "Failed to read entity multisource CSV headers",
            json!({
                "stage": "link",
                "source": source.name,
                "path": source.rows_path,
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    ensure_no_reserved_columns(&headers, source)?;
    ensure_configured_columns(&headers, source)?;
    let records = reader
        .records()
        .map(|record| {
            record.map_err(|error| {
                input_refusal(
                    "Failed to parse entity multisource CSV record",
                    json!({
                        "stage": "link",
                        "source": source.name,
                        "path": source.rows_path,
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SourceRows {
        source: source.clone(),
        headers,
        records,
    })
}

fn write_materialized_rows(sources: &[SourceRows], output: &Path) -> Result<(), Refusal> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            io_refusal(
                "Failed to create entity multisource materialization directory",
                output,
                error,
            )
        })?;
    }
    let file = File::create(output).map_err(|error| {
        io_refusal(
            "Failed to create entity multisource materialized rows",
            output,
            error,
        )
    })?;
    let mut writer = WriterBuilder::new().from_writer(file);
    let merged_headers = merged_headers(sources);
    let mut output_headers = merged_headers.clone();
    output_headers.push(LINK_SOURCE_NAME_COLUMN.to_string());
    output_headers.push(LINK_SIDE_COLUMN.to_string());
    output_headers.push(LINK_SOURCE_ROW_COLUMN.to_string());
    output_headers.push(LINK_SOURCE_ORDINAL_COLUMN.to_string());
    writer.write_record(&output_headers).map_err(|error| {
        io_refusal(
            "Failed to write entity multisource materialized headers",
            output,
            error,
        )
    })?;

    for source in sources {
        let header_index = source.header_index();
        for (ordinal, record) in source.records.iter().enumerate() {
            let mut row = merged_headers
                .iter()
                .map(|header| {
                    header_index
                        .get(header)
                        .and_then(|index| record.get(*index))
                        .unwrap_or("")
                        .to_string()
                })
                .collect::<Vec<_>>();
            let ordinal = ordinal as u64 + 1;
            row.push(source.source.name.clone());
            row.push(source.source.role.as_str().to_string());
            row.push(source_row_id(source, record, ordinal));
            row.push(ordinal.to_string());
            writer.write_record(&row).map_err(|error| {
                io_refusal(
                    "Failed to write entity multisource materialized row",
                    output,
                    error,
                )
            })?;
        }
    }
    writer.flush().map_err(|error| {
        io_refusal(
            "Failed to flush entity multisource materialized rows",
            output,
            error,
        )
    })
}

fn detect_anchor_conflicts(sources: &[SourceRows]) -> EntityMultisourceConsistency {
    let mut anchors = BTreeMap::<String, Vec<EntitySourceRowRef>>::new();
    for source in sources {
        let (Some(namespace), Some(anchor_column), Some(canonical_id_column)) = (
            source.source.anchor_namespace.as_deref(),
            source.source.anchor_column.as_deref(),
            source.source.canonical_id_column.as_deref(),
        ) else {
            continue;
        };
        let header_index = source.header_index();
        let Some(anchor_index) = header_index.get(anchor_column).copied() else {
            continue;
        };
        let Some(canonical_index) = header_index.get(canonical_id_column).copied() else {
            continue;
        };
        for (ordinal, record) in source.records.iter().enumerate() {
            let Some(anchor_value) = record.get(anchor_index).map(str::trim) else {
                continue;
            };
            let Some(canonical_id) = record.get(canonical_index).map(str::trim) else {
                continue;
            };
            if anchor_value.is_empty() || canonical_id.is_empty() {
                continue;
            }
            anchors
                .entry(format!("{namespace}:{anchor_value}"))
                .or_default()
                .push(EntitySourceRowRef {
                    source: source.source.name.clone(),
                    row_id: source_row_id(source, record, ordinal as u64 + 1),
                    canonical_id: canonical_id.to_string(),
                });
        }
    }

    let mut anchor_conflicts = anchors
        .into_iter()
        .filter_map(|(anchor_key, mut source_rows)| {
            source_rows.sort_by(|left, right| {
                left.source
                    .cmp(&right.source)
                    .then_with(|| left.row_id.cmp(&right.row_id))
                    .then_with(|| left.canonical_id.cmp(&right.canonical_id))
            });
            let canonical_ids = source_rows
                .iter()
                .map(|row| row.canonical_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            (canonical_ids.len() > 1).then_some(EntityAnchorConflict {
                anchor_key,
                canonical_ids,
                source_rows,
            })
        })
        .collect::<Vec<_>>();
    anchor_conflicts.sort_by(|left, right| left.anchor_key.cmp(&right.anchor_key));

    let abstentions = anchor_conflicts
        .iter()
        .map(|conflict| EntityMultisourceAbstention {
            reason: EntityMultisourceAbstentionReason::AnchorConflict,
            anchor_key: conflict.anchor_key.clone(),
            message: "conflicting exact anchors across sources; abstain rather than force a transitive merge".to_string(),
        })
        .collect();

    EntityMultisourceConsistency {
        anchor_conflicts,
        abstentions,
    }
}

fn ensure_no_reserved_columns(
    headers: &StringRecord,
    source: &NormalizedSource,
) -> Result<(), Refusal> {
    for reserved in [
        LINK_SOURCE_NAME_COLUMN,
        LINK_SIDE_COLUMN,
        LINK_SOURCE_ROW_COLUMN,
        LINK_SOURCE_ORDINAL_COLUMN,
    ] {
        if headers.iter().any(|header| header == reserved) {
            return Err(input_refusal(
                "Entity multisource input already contains reserved link metadata columns",
                json!({
                    "stage": "link",
                    "reason": "reserved_column",
                    "source": source.name,
                    "path": source.rows_path,
                    "column": reserved,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn ensure_configured_columns(
    headers: &StringRecord,
    source: &NormalizedSource,
) -> Result<(), Refusal> {
    for column in [
        source.local_id_column.as_deref(),
        source.anchor_column.as_deref(),
        source.canonical_id_column.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !headers.iter().any(|header| header == column) {
            return Err(input_refusal(
                "Entity multisource configured column is missing",
                json!({
                    "stage": "link",
                    "reason": "missing_configured_column",
                    "source": source.name,
                    "path": source.rows_path,
                    "column": column,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn merged_headers(sources: &[SourceRows]) -> Vec<String> {
    sources
        .iter()
        .flat_map(|source| source.headers.iter())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn source_row_id(source: &SourceRows, record: &StringRecord, ordinal: u64) -> String {
    let header_index = source.header_index();
    source
        .source
        .local_id_column
        .as_deref()
        .or(Some("source_row_id"))
        .and_then(|column| header_index.get(column))
        .and_then(|index| record.get(*index))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| ordinal.to_string())
}

fn normalize_source_name(value: &str) -> Result<String, Refusal> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(input_refusal(
            "Entity multisource source name is required",
            json!({
                "stage": "link",
                "reason": "empty_source_name",
                "writes_performed": false
            }),
        ));
    }
    if !normalized
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(input_refusal(
            "Entity multisource source names may contain only ASCII letters, digits, '.', '_' or '-'",
            json!({
                "stage": "link",
                "reason": "invalid_source_name",
                "source": normalized,
                "writes_performed": false
            }),
        ));
    }
    Ok(normalized.to_string())
}

fn normalize_column_name(value: &str) -> Result<String, Refusal> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(input_refusal(
            "Entity multisource column name is required",
            json!({
                "stage": "link",
                "reason": "empty_column_name",
                "writes_performed": false
            }),
        ));
    }
    Ok(normalized.to_string())
}

fn normalize_anchor_namespace(value: &str) -> Result<String, Refusal> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(input_refusal(
            "Entity multisource anchor namespace is required",
            json!({
                "stage": "link",
                "reason": "empty_anchor_namespace",
                "writes_performed": false
            }),
        ));
    }
    Ok(normalized.to_string())
}

fn input_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::InputContract.to_refusal(
        message,
        detail,
        Some("Fix entity multisource link inputs, then rerun canon entity link".to_string()),
    )
}

fn budget_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::CandidateBudget.to_refusal(
        message,
        detail,
        Some("Reduce the comparison graph or raise pair budgets explicitly".to_string()),
    )
}

fn io_refusal(message: &'static str, path: &Path, error: impl std::fmt::Display) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        message,
        json!({
            "stage": "link",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        Some("Check entity multisource link work-dir permissions, then rerun".to_string()),
    )
}
