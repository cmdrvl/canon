#![forbid(unsafe_code)]

use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::Path,
};

pub const EXPORT_PROJECTION_VERSION: &str = "canon.export.projection.v1";

pub type ProjectionResult<T> = Result<T, ProjectionError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionErrorCode {
    UnsupportedVersion,
    EmptyField,
    InvalidDigest,
    UnsafeIdentifier,
    ReservedTableCollision,
    DuplicateTable,
    DuplicateColumn,
    UnknownColumn,
    UnsafeExpression,
    NondeterministicExpression,
    MissingField,
    BoundaryViolation,
    IncompatibleSnapshot,
    Io,
    Sqlite,
    Csv,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionError {
    pub code: ProjectionErrorCode,
    pub message: String,
}

impl ProjectionError {
    fn new(code: ProjectionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProjectionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionRowKind {
    Identity,
    Identifier,
    Relation,
    Assignment,
    Provenance,
    DomainView,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionColumnType {
    Text,
    IntegerText,
    Digest,
    CanonicalId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionSearchMode {
    Exact,
    Prefix,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionPackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    pub package_digest: String,
    pub registry_snapshot_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<ProjectionSourceRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<ProjectionTable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionSourceRecord {
    pub record_id: String,
    pub row_kind: ProjectionRowKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionTable {
    pub table_name: String,
    pub row_kind: ProjectionRowKind,
    pub columns: Vec<ProjectionColumn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_key: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search_fields: Vec<ProjectionSearchField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionColumn {
    pub name: String,
    pub column_type: ProjectionColumnType,
    pub expression: ProjectionExpression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionSearchField {
    pub field_name: String,
    pub column: String,
    pub mode: ProjectionSearchMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectionExpression {
    Field {
        name: String,
    },
    Literal {
        value: String,
    },
    CanonicalId,
    RelationId,
    AssignmentId,
    SourceRecordId,
    PackageDigest,
    RegistrySnapshotDigest,
    RedactedField {
        name: String,
        keep_last: usize,
    },
    Concat {
        separator: String,
        parts: Vec<ProjectionExpression>,
    },
    Lowercase {
        expr: Box<ProjectionExpression>,
    },
    UnsafeSql {
        sql: String,
    },
    Now,
    RandomUuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionExportPlan {
    pub version: String,
    pub registry_snapshot_digest: String,
    pub package_digests: Vec<String>,
    pub tables: Vec<CompiledProjectionTable>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledProjectionTable {
    pub table_name: String,
    pub row_kind: ProjectionRowKind,
    pub columns: Vec<CompiledProjectionColumn>,
    pub primary_key: Vec<String>,
    pub search_fields: Vec<ProjectionSearchField>,
    pub rows: Vec<CompiledProjectionRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledProjectionColumn {
    pub name: String,
    pub column_type: ProjectionColumnType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledProjectionRow {
    pub row_id: String,
    pub package_id: String,
    pub package_digest: String,
    pub values: BTreeMap<String, String>,
}

pub fn export_projection_schema_version() -> &'static str {
    EXPORT_PROJECTION_VERSION
}

pub fn plan_projection_exports(
    mut packages: Vec<ProjectionPackage>,
) -> ProjectionResult<ProjectionExportPlan> {
    if packages.is_empty() {
        return Err(ProjectionError::new(
            ProjectionErrorCode::EmptyField,
            "at least one projection package is required",
        ));
    }

    for package in &mut packages {
        normalize_package(package)?;
    }
    packages.sort_by(|left, right| {
        (
            &left.package_id,
            &left.package_version,
            &left.package_digest,
        )
            .cmp(&(
                &right.package_id,
                &right.package_version,
                &right.package_digest,
            ))
    });

    let registry_snapshot_digest = packages[0].registry_snapshot_digest.clone();
    if packages
        .iter()
        .any(|package| package.registry_snapshot_digest != registry_snapshot_digest)
    {
        return Err(ProjectionError::new(
            ProjectionErrorCode::IncompatibleSnapshot,
            "all projection packages must bind the same registry snapshot digest",
        ));
    }

    let mut seen_tables = BTreeSet::new();
    let mut compiled_tables = Vec::new();
    for package in &packages {
        for table in &package.tables {
            if reserved_table_names().contains(table.table_name.as_str()) {
                return Err(ProjectionError::new(
                    ProjectionErrorCode::ReservedTableCollision,
                    format!(
                        "projection table {} collides with a base export table",
                        table.table_name
                    ),
                ));
            }
            if !seen_tables.insert(table.table_name.clone()) {
                return Err(ProjectionError::new(
                    ProjectionErrorCode::DuplicateTable,
                    format!("duplicate projection table {}", table.table_name),
                ));
            }
            compiled_tables.push(compile_table(package, table)?);
        }
    }
    compiled_tables.sort_by(|left, right| left.table_name.cmp(&right.table_name));

    let package_digests = packages
        .iter()
        .map(|package| package.package_digest.clone())
        .collect::<Vec<_>>();
    let mut plan = ProjectionExportPlan {
        version: EXPORT_PROJECTION_VERSION.to_string(),
        registry_snapshot_digest,
        package_digests,
        tables: compiled_tables,
        content_hash: String::new(),
    };
    plan.content_hash = plan_hash(&plan)?;
    Ok(plan)
}

pub fn canonical_projection_plan_bytes(plan: &ProjectionExportPlan) -> ProjectionResult<Vec<u8>> {
    serde_json::to_vec(plan).map_err(|error| {
        ProjectionError::new(
            ProjectionErrorCode::Serialization,
            format!("failed to serialize projection plan: {error}"),
        )
    })
}

pub fn write_dbt_projection_seeds(
    plan: &ProjectionExportPlan,
    output_dir: &Path,
) -> ProjectionResult<Vec<String>> {
    fs::create_dir_all(output_dir).map_err(|error| {
        ProjectionError::new(
            ProjectionErrorCode::Io,
            format!("failed to create dbt projection output directory: {error}"),
        )
    })?;

    let mut written = Vec::new();
    for table in &plan.tables {
        let path = output_dir.join(format!("{}.csv", table.table_name));
        let mut writer = csv::Writer::from_path(&path).map_err(|error| {
            ProjectionError::new(
                ProjectionErrorCode::Csv,
                format!(
                    "failed to create dbt projection seed {}: {error}",
                    path.display()
                ),
            )
        })?;

        let header = table
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>();
        writer.write_record(header).map_err(|error| {
            ProjectionError::new(
                ProjectionErrorCode::Csv,
                format!(
                    "failed to write dbt projection header {}: {error}",
                    path.display()
                ),
            )
        })?;

        for row in &table.rows {
            let record = table
                .columns
                .iter()
                .map(|column| row.values.get(&column.name).cloned().unwrap_or_default())
                .collect::<Vec<_>>();
            writer.write_record(record).map_err(|error| {
                ProjectionError::new(
                    ProjectionErrorCode::Csv,
                    format!(
                        "failed to write dbt projection row {}: {error}",
                        path.display()
                    ),
                )
            })?;
        }
        writer.flush().map_err(|error| {
            ProjectionError::new(
                ProjectionErrorCode::Csv,
                format!(
                    "failed to flush dbt projection seed {}: {error}",
                    path.display()
                ),
            )
        })?;
        written.push(path.display().to_string());
    }
    written.sort();
    Ok(written)
}

pub fn write_sqlite_projection_index(
    plan: &ProjectionExportPlan,
    path: &Path,
) -> ProjectionResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ProjectionError::new(
                ProjectionErrorCode::Io,
                format!("failed to create sqlite projection parent directory: {error}"),
            )
        })?;
    }

    let conn = Connection::open(path).map_err(|error| {
        ProjectionError::new(
            ProjectionErrorCode::Sqlite,
            format!(
                "failed to create sqlite projection index {}: {error}",
                path.display()
            ),
        )
    })?;
    conn.execute_batch(
        "begin;
         create table if not exists projection_metadata (
           key text primary key,
           value text not null
         );
         create table if not exists projection_search_fields (
           table_name text not null,
           field_name text not null,
           column_name text not null,
           mode text not null,
           primary key (table_name, field_name)
         );
         commit;",
    )
    .map_err(|error| {
        ProjectionError::new(
            ProjectionErrorCode::Sqlite,
            format!("failed to initialize sqlite projection metadata: {error}"),
        )
    })?;
    insert_metadata(&conn, plan)?;

    for table in &plan.tables {
        create_sqlite_table(&conn, table)?;
        insert_sqlite_rows(&conn, table)?;
        insert_search_fields(&conn, table)?;
    }
    Ok(())
}

fn normalize_package(package: &mut ProjectionPackage) -> ProjectionResult<()> {
    package.version = package.version.trim().to_string();
    if package.version != EXPORT_PROJECTION_VERSION {
        return Err(ProjectionError::new(
            ProjectionErrorCode::UnsupportedVersion,
            "unsupported export projection package version",
        ));
    }
    package.package_id = normalized_non_empty(&package.package_id, "package_id")?;
    package.package_version = normalized_non_empty(&package.package_version, "package_version")?;
    package.package_digest = normalized_digest(&package.package_digest, "package_digest")?;
    package.registry_snapshot_digest = normalized_digest(
        &package.registry_snapshot_digest,
        "registry_snapshot_digest",
    )?;

    let mut seen_records = BTreeSet::new();
    for record in &mut package.records {
        normalize_record(record)?;
        if !seen_records.insert(record.record_id.clone()) {
            return Err(ProjectionError::new(
                ProjectionErrorCode::DuplicateTable,
                format!("duplicate source record {}", record.record_id),
            ));
        }
    }
    package
        .records
        .sort_by(|left, right| left.record_id.cmp(&right.record_id));

    let mut seen_tables = BTreeSet::new();
    for table in &mut package.tables {
        normalize_table(table)?;
        if !seen_tables.insert(table.table_name.clone()) {
            return Err(ProjectionError::new(
                ProjectionErrorCode::DuplicateTable,
                format!("duplicate projection table {}", table.table_name),
            ));
        }
    }
    package
        .tables
        .sort_by(|left, right| left.table_name.cmp(&right.table_name));
    Ok(())
}

fn normalize_record(record: &mut ProjectionSourceRecord) -> ProjectionResult<()> {
    record.record_id = normalized_non_empty(&record.record_id, "record_id")?;
    record.canonical_id = normalize_optional(record.canonical_id.take());
    record.relation_id = normalize_optional(record.relation_id.take());
    record.assignment_id = normalize_optional(record.assignment_id.take());
    record.provenance_ref = normalize_optional(record.provenance_ref.take());

    let mut fields = BTreeMap::new();
    for (key, value) in std::mem::take(&mut record.fields) {
        fields.insert(normalized_safe_field_name(&key)?, value.trim().to_string());
    }
    record.fields = fields;
    validate_record_boundary(record)
}

fn normalize_table(table: &mut ProjectionTable) -> ProjectionResult<()> {
    table.table_name = normalized_sql_identifier(&table.table_name)?;
    if table.columns.is_empty() {
        return Err(ProjectionError::new(
            ProjectionErrorCode::EmptyField,
            format!(
                "projection table {} must declare at least one column",
                table.table_name
            ),
        ));
    }

    let mut column_names = BTreeSet::new();
    for column in &mut table.columns {
        column.name = normalized_sql_identifier(&column.name)?;
        if !column_names.insert(column.name.clone()) {
            return Err(ProjectionError::new(
                ProjectionErrorCode::DuplicateColumn,
                format!(
                    "duplicate column {} in table {}",
                    column.name, table.table_name
                ),
            ));
        }
        validate_expression(&column.expression)?;
    }

    for key in &mut table.primary_key {
        *key = normalized_sql_identifier(key)?;
        if !column_names.contains(key) {
            return Err(ProjectionError::new(
                ProjectionErrorCode::UnknownColumn,
                format!(
                    "primary key column {key} is not declared on table {}",
                    table.table_name
                ),
            ));
        }
    }
    table.primary_key.sort();
    table.primary_key.dedup();

    let mut fields = BTreeSet::new();
    for search_field in &mut table.search_fields {
        search_field.field_name = normalized_sql_identifier(&search_field.field_name)?;
        search_field.column = normalized_sql_identifier(&search_field.column)?;
        if !column_names.contains(&search_field.column) {
            return Err(ProjectionError::new(
                ProjectionErrorCode::UnknownColumn,
                format!(
                    "search field {} references missing column {}",
                    search_field.field_name, search_field.column
                ),
            ));
        }
        if !fields.insert(search_field.field_name.clone()) {
            return Err(ProjectionError::new(
                ProjectionErrorCode::DuplicateColumn,
                format!("duplicate search field {}", search_field.field_name),
            ));
        }
    }
    table
        .search_fields
        .sort_by(|left, right| left.field_name.cmp(&right.field_name));
    Ok(())
}

fn validate_record_boundary(record: &ProjectionSourceRecord) -> ProjectionResult<()> {
    match record.row_kind {
        ProjectionRowKind::Identity | ProjectionRowKind::Identifier => {
            if record.relation_id.is_some() || record.assignment_id.is_some() {
                return Err(boundary_error(
                    "identity/identifier projection records cannot carry relation or assignment ids",
                ));
            }
        }
        ProjectionRowKind::Relation => {
            if record.relation_id.is_none() || record.assignment_id.is_some() {
                return Err(boundary_error(
                    "relation projection records require relation_id and cannot carry assignment_id",
                ));
            }
        }
        ProjectionRowKind::Assignment => {
            if record.assignment_id.is_none() || record.relation_id.is_some() {
                return Err(boundary_error(
                    "assignment projection records require assignment_id and cannot carry relation_id",
                ));
            }
        }
        ProjectionRowKind::Provenance
        | ProjectionRowKind::DomainView
        | ProjectionRowKind::Unresolved => {}
    }
    Ok(())
}

fn validate_expression(expression: &ProjectionExpression) -> ProjectionResult<()> {
    match expression {
        ProjectionExpression::UnsafeSql { .. } => Err(ProjectionError::new(
            ProjectionErrorCode::UnsafeExpression,
            "raw SQL expressions are not accepted in projection packages",
        )),
        ProjectionExpression::Now | ProjectionExpression::RandomUuid => Err(ProjectionError::new(
            ProjectionErrorCode::NondeterministicExpression,
            "nondeterministic projection expressions are not accepted",
        )),
        ProjectionExpression::Field { name } | ProjectionExpression::RedactedField { name, .. } => {
            normalized_safe_field_name(name).map(|_| ())
        }
        ProjectionExpression::Literal { .. }
        | ProjectionExpression::CanonicalId
        | ProjectionExpression::RelationId
        | ProjectionExpression::AssignmentId
        | ProjectionExpression::SourceRecordId
        | ProjectionExpression::PackageDigest
        | ProjectionExpression::RegistrySnapshotDigest => Ok(()),
        ProjectionExpression::Concat { parts, .. } => {
            if parts.is_empty() {
                return Err(ProjectionError::new(
                    ProjectionErrorCode::EmptyField,
                    "concat expressions require at least one part",
                ));
            }
            for part in parts {
                validate_expression(part)?;
            }
            Ok(())
        }
        ProjectionExpression::Lowercase { expr } => validate_expression(expr),
    }
}

fn compile_table(
    package: &ProjectionPackage,
    table: &ProjectionTable,
) -> ProjectionResult<CompiledProjectionTable> {
    let mut rows = Vec::new();
    for record in package
        .records
        .iter()
        .filter(|record| record.row_kind == table.row_kind)
    {
        let mut values = BTreeMap::new();
        for column in &table.columns {
            values.insert(
                column.name.clone(),
                eval_expression(&column.expression, package, record)?,
            );
        }
        let row_id = values
            .get("projection_row_id")
            .cloned()
            .unwrap_or_else(|| format!("{}:{}", package.package_id, record.record_id));
        rows.push(CompiledProjectionRow {
            row_id,
            package_id: package.package_id.clone(),
            package_digest: package.package_digest.clone(),
            values,
        });
    }
    rows.sort_by(|left, right| left.row_id.cmp(&right.row_id));

    Ok(CompiledProjectionTable {
        table_name: table.table_name.clone(),
        row_kind: table.row_kind,
        columns: table
            .columns
            .iter()
            .map(|column| CompiledProjectionColumn {
                name: column.name.clone(),
                column_type: column.column_type,
            })
            .collect(),
        primary_key: table.primary_key.clone(),
        search_fields: table.search_fields.clone(),
        rows,
    })
}

fn eval_expression(
    expression: &ProjectionExpression,
    package: &ProjectionPackage,
    record: &ProjectionSourceRecord,
) -> ProjectionResult<String> {
    match expression {
        ProjectionExpression::Field { name } => record.fields.get(name).cloned().ok_or_else(|| {
            ProjectionError::new(
                ProjectionErrorCode::MissingField,
                format!("record {} is missing field {}", record.record_id, name),
            )
        }),
        ProjectionExpression::Literal { value } => Ok(value.clone()),
        ProjectionExpression::CanonicalId => record.canonical_id.clone().ok_or_else(|| {
            ProjectionError::new(
                ProjectionErrorCode::BoundaryViolation,
                format!("record {} has no canonical_id", record.record_id),
            )
        }),
        ProjectionExpression::RelationId => {
            if record.row_kind != ProjectionRowKind::Relation {
                return Err(boundary_error(
                    "relation_id expressions are only valid for relation tables",
                ));
            }
            record.relation_id.clone().ok_or_else(|| {
                ProjectionError::new(
                    ProjectionErrorCode::BoundaryViolation,
                    format!("record {} has no relation_id", record.record_id),
                )
            })
        }
        ProjectionExpression::AssignmentId => {
            if record.row_kind != ProjectionRowKind::Assignment {
                return Err(boundary_error(
                    "assignment_id expressions are only valid for assignment tables",
                ));
            }
            record.assignment_id.clone().ok_or_else(|| {
                ProjectionError::new(
                    ProjectionErrorCode::BoundaryViolation,
                    format!("record {} has no assignment_id", record.record_id),
                )
            })
        }
        ProjectionExpression::SourceRecordId => Ok(record.record_id.clone()),
        ProjectionExpression::PackageDigest => Ok(package.package_digest.clone()),
        ProjectionExpression::RegistrySnapshotDigest => {
            Ok(package.registry_snapshot_digest.clone())
        }
        ProjectionExpression::RedactedField { name, keep_last } => {
            let value = record.fields.get(name).cloned().ok_or_else(|| {
                ProjectionError::new(
                    ProjectionErrorCode::MissingField,
                    format!("record {} is missing field {}", record.record_id, name),
                )
            })?;
            Ok(mask_last(&value, *keep_last))
        }
        ProjectionExpression::Concat { separator, parts } => {
            let mut values = Vec::with_capacity(parts.len());
            for part in parts {
                values.push(eval_expression(part, package, record)?);
            }
            Ok(values.join(separator))
        }
        ProjectionExpression::Lowercase { expr } => {
            Ok(eval_expression(expr, package, record)?.to_ascii_lowercase())
        }
        ProjectionExpression::UnsafeSql { .. } => Err(ProjectionError::new(
            ProjectionErrorCode::UnsafeExpression,
            "raw SQL expressions are not accepted in projection packages",
        )),
        ProjectionExpression::Now | ProjectionExpression::RandomUuid => Err(ProjectionError::new(
            ProjectionErrorCode::NondeterministicExpression,
            "nondeterministic projection expressions are not accepted",
        )),
    }
}

fn insert_metadata(conn: &Connection, plan: &ProjectionExportPlan) -> ProjectionResult<()> {
    let metadata = [
        ("version", plan.version.clone()),
        (
            "registry_snapshot_digest",
            plan.registry_snapshot_digest.clone(),
        ),
        ("content_hash", plan.content_hash.clone()),
        ("table_count", plan.tables.len().to_string()),
    ];
    for (key, value) in metadata {
        conn.execute(
            "insert or replace into projection_metadata (key, value) values (?1, ?2)",
            [key, value.as_str()],
        )
        .map_err(sqlite_error)?;
    }
    Ok(())
}

fn create_sqlite_table(conn: &Connection, table: &CompiledProjectionTable) -> ProjectionResult<()> {
    let columns = table
        .columns
        .iter()
        .map(|column| format!("{} text not null", column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let primary_key = if table.primary_key.is_empty() {
        String::new()
    } else {
        format!(", primary key ({})", table.primary_key.join(", "))
    };
    conn.execute(
        &format!("create table {} ({columns}{primary_key})", table.table_name),
        [],
    )
    .map_err(sqlite_error)?;
    Ok(())
}

fn insert_sqlite_rows(conn: &Connection, table: &CompiledProjectionTable) -> ProjectionResult<()> {
    let column_names = table
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();
    let placeholders = (0..column_names.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "insert into {} ({}) values ({})",
        table.table_name,
        column_names.join(", "),
        placeholders
    );
    let mut statement = conn.prepare(&sql).map_err(sqlite_error)?;
    for row in &table.rows {
        let values = column_names
            .iter()
            .map(|column| row.values[column].clone())
            .collect::<Vec<_>>();
        statement
            .execute(params_from_iter(values.iter()))
            .map_err(sqlite_error)?;
    }
    Ok(())
}

fn insert_search_fields(
    conn: &Connection,
    table: &CompiledProjectionTable,
) -> ProjectionResult<()> {
    for field in &table.search_fields {
        conn.execute(
            "insert into projection_search_fields (table_name, field_name, column_name, mode) values (?1, ?2, ?3, ?4)",
            [
                table.table_name.as_str(),
                field.field_name.as_str(),
                field.column.as_str(),
                search_mode_label(field.mode),
            ],
        )
        .map_err(sqlite_error)?;
    }
    Ok(())
}

fn plan_hash(plan: &ProjectionExportPlan) -> ProjectionResult<String> {
    let mut canonical = plan.clone();
    canonical.content_hash.clear();
    let bytes = canonical_projection_plan_bytes(&canonical)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn normalized_non_empty(value: &str, field: &str) -> ProjectionResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ProjectionError::new(
            ProjectionErrorCode::EmptyField,
            format!("{field} cannot be empty"),
        ));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|inner| inner.trim().to_string())
        .filter(|inner| !inner.is_empty())
}

fn normalized_digest(value: &str, field: &str) -> ProjectionResult<String> {
    let digest = normalized_non_empty(value, field)?;
    let Some(hex) = digest.strip_prefix("blake3:") else {
        return Err(ProjectionError::new(
            ProjectionErrorCode::InvalidDigest,
            format!("{field} must be a blake3 digest"),
        ));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProjectionError::new(
            ProjectionErrorCode::InvalidDigest,
            format!("{field} must be a 64-character blake3 hex digest"),
        ));
    }
    Ok(format!("blake3:{}", hex.to_ascii_lowercase()))
}

fn normalized_sql_identifier(value: &str) -> ProjectionResult<String> {
    let value = normalized_non_empty(value, "sql_identifier")?;
    let bytes = value.as_bytes();
    if bytes.len() > 63
        || !bytes[0].is_ascii_lowercase()
        || bytes
            .iter()
            .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_'))
        || sql_keywords().contains(value.as_str())
    {
        return Err(ProjectionError::new(
            ProjectionErrorCode::UnsafeIdentifier,
            format!("unsafe SQL identifier: {value}"),
        ));
    }
    Ok(value)
}

fn normalized_safe_field_name(value: &str) -> ProjectionResult<String> {
    let value = normalized_non_empty(value, "field_name")?;
    if value.len() > 128
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')))
    {
        return Err(ProjectionError::new(
            ProjectionErrorCode::UnsafeIdentifier,
            format!("unsafe field name: {value}"),
        ));
    }
    Ok(value)
}

fn mask_last(value: &str, keep_last: usize) -> String {
    if keep_last == 0 {
        return "[REDACTED]".to_string();
    }
    let len = value.chars().count();
    if len <= keep_last {
        return value.to_string();
    }
    let suffix = value
        .chars()
        .skip(len.saturating_sub(keep_last))
        .collect::<String>();
    format!("[REDACTED]{suffix}")
}

fn boundary_error(message: &str) -> ProjectionError {
    ProjectionError::new(ProjectionErrorCode::BoundaryViolation, message)
}

fn sqlite_error(error: rusqlite::Error) -> ProjectionError {
    ProjectionError::new(
        ProjectionErrorCode::Sqlite,
        format!("sqlite projection export failed: {error}"),
    )
}

fn search_mode_label(mode: ProjectionSearchMode) -> &'static str {
    match mode {
        ProjectionSearchMode::Exact => "exact",
        ProjectionSearchMode::Prefix => "prefix",
        ProjectionSearchMode::Text => "text",
    }
}

fn reserved_table_names() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "aliases",
        "aliases_fts",
        "entities",
        "external_keys",
        "field_weights",
        "metadata",
        "scoring_tiers",
        "projection_metadata",
        "projection_search_fields",
    ])
}

fn sql_keywords() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "alter", "create", "delete", "drop", "from", "group", "insert", "join", "order", "select",
        "table", "union", "update", "where",
    ])
}
