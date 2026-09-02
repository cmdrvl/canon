use super::{
    MappingFile, load_registry_definition,
    package::{REGISTRY_PACKAGE_SCHEMA_VERSION, compile_registry_package},
};
use crate::{Refusal, RefusalCode, RegistryMeta};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::{Path, PathBuf},
};

const EXPORT_VERSION: &str = "canon_registry_export.v0";
const SEARCH_INDEX_ARTIFACT_VERSION: &str = "canon_registry_search_index.v0";
const SEARCH_INDEX_SCHEMA_VERSION: &str = "canon_registry_search_index_schema.v1";
const NORMALIZATION_SPEC_ID: &str = "canon_registry_search_key.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryExportFormat {
    DbtSeed,
    SearchIndex,
}

impl RegistryExportFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::DbtSeed => "dbt-seed",
            Self::SearchIndex => "search-index",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegistryExportRequest {
    pub registry: PathBuf,
    pub format: RegistryExportFormat,
    pub out: PathBuf,
    pub namespace: Option<String>,
    pub source_files: Vec<String>,
    pub canonical_types: Vec<String>,
    pub rule_id_prefixes: Vec<String>,
    pub canonical_iri_prefix: String,
    pub schema_out: Option<PathBuf>,
    pub anti_collapse_test_out: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryExportFilters {
    pub namespace: Option<String>,
    pub source_files: Vec<String>,
    pub canonical_types: Vec<String>,
    pub rule_id_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RegistryExportSummary {
    pub source_entry_count: usize,
    pub filtered_entry_count: usize,
    pub exported_alias_count: usize,
    pub exported_entity_count: usize,
    pub skipped_filter_count: usize,
    pub skipped_shadowed_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryExportOutput {
    pub version: String,
    pub format: String,
    pub registry: RegistryMeta,
    pub output_path: String,
    pub content_hash: String,
    pub normalization_spec: String,
    pub filters: RegistryExportFilters,
    pub summary: RegistryExportSummary,
    pub files: Vec<String>,
}

impl RegistryExportOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{} {} export: {} aliases, {} entities -> {} ({})",
            self.registry.id,
            self.format,
            self.summary.exported_alias_count,
            self.summary.exported_entity_count,
            self.output_path,
            self.content_hash
        )
    }

    pub fn temporal_projection_contract_json(
        &self,
        compiled_snapshot_digest: &str,
        valid_at: &str,
        known_as_of: &str,
        scope_ref: Option<&str>,
    ) -> Result<serde_json::Value, Refusal> {
        let compiled_snapshot_digest =
            normalized_blake3_digest(compiled_snapshot_digest, "compiled_snapshot_digest")?;
        let temporal = normalize_export_temporal_scope(valid_at, known_as_of)?;
        let scope_ref = scope_ref
            .map(|scope| normalized_non_empty(scope, "scope_ref"))
            .transpose()?;
        let current_format_preserves_relationship_validity =
            matches!(self.format.as_str(), "dbt-seed" | "search-index");

        Ok(json!({
            "version": "canon.registry.temporal_projection_contract.v1",
            "registry": {
                "id": self.registry.id,
                "version": self.registry.version,
            },
            "format": self.format,
            "content_hash": self.content_hash,
            "compiled_snapshot_digest": compiled_snapshot_digest,
            "valid_at": temporal.valid_at,
            "known_as_of": temporal.known_as_of,
            "mode": temporal.mode,
            "calendar": "gregorian",
            "timezone": "UTC",
            "precision": temporal.precision,
            "date_only_values_are_fabricated": false,
            "scope_ref": scope_ref,
            "current_format": {
                "preserves_compiled_snapshot_identity": true,
                "preserves_relationship_validity": current_format_preserves_relationship_validity,
                "relationship_validity_contract": if current_format_preserves_relationship_validity {
                    "projection rows identify the compiled snapshot and relationship sidecars retain valid-time/known-time intervals"
                } else {
                    "format is represented as a planned projection contract only"
                },
            },
            "portable_projection_formats": [
                {
                    "format": "dbt-seed",
                    "compiled_snapshot_fields": ["compiled_snapshot_digest", "valid_at", "known_as_of", "scope_ref"],
                    "relationship_validity": "sidecar_reference"
                },
                {
                    "format": "search-index",
                    "compiled_snapshot_fields": ["metadata.compiled_snapshot_digest", "metadata.valid_at", "metadata.known_as_of", "metadata.scope_ref"],
                    "relationship_validity": "metadata_and_relation_sidecar_reference"
                },
                {
                    "format": "parquet",
                    "compiled_snapshot_fields": ["compiled_snapshot_digest", "valid_at", "known_as_of", "scope_ref"],
                    "relationship_validity": "typed_interval_columns"
                },
                {
                    "format": "rdf",
                    "compiled_snapshot_fields": ["canon:compiledSnapshotDigest", "canon:validAt", "canon:knownAsOf", "canon:scopeRef"],
                    "relationship_validity": "valid_time_named_graph_or_reified_interval"
                }
            ]
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExportTemporalScope {
    mode: &'static str,
    valid_at: String,
    known_as_of: String,
    precision: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ExportRow {
    namespace: Option<String>,
    input: String,
    normalized_key: String,
    canonical_id: String,
    canonical_iri: String,
    canonical_type: String,
    alias_kind: String,
    rule_id: String,
    match_source: String,
    registry_id: String,
    registry_version: String,
    source_file: String,
    entry_order: usize,
}

#[derive(Debug, Clone, Serialize)]
struct EntityRow {
    canonical_id: String,
    canonical_iri: String,
    canonical_type: String,
    display_name: String,
    normalized_display_key: String,
    alias_count: usize,
    #[serde(skip_serializing)]
    display_rank: u8,
}

#[derive(Debug, Clone)]
struct RegistryExportSnapshot {
    registry: RegistryMeta,
    package: RegistryExportPackage,
    filters: RegistryExportFilters,
    source_entry_count: usize,
    filtered_entry_count: usize,
    skipped_filter_count: usize,
    skipped_shadowed_count: usize,
    rows: Vec<ExportRow>,
    entities: Vec<EntityRow>,
    snapshot_hash: String,
}

#[derive(Debug, Clone, Serialize)]
struct RegistryExportPackage {
    schema_version: String,
    id: String,
    version: String,
    content_digest: String,
    entry_count: usize,
    effective_mapping_count: usize,
}

impl RegistryExportSnapshot {
    fn summary(&self) -> RegistryExportSummary {
        RegistryExportSummary {
            source_entry_count: self.source_entry_count,
            filtered_entry_count: self.filtered_entry_count,
            exported_alias_count: self.rows.len(),
            exported_entity_count: self.entities.len(),
            skipped_filter_count: self.skipped_filter_count,
            skipped_shadowed_count: self.skipped_shadowed_count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryExportPlannedFileRole {
    PrimaryArtifact,
    DbtSchema,
    DbtAntiCollapseTest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryExportPlannedFile {
    path: PathBuf,
    role: RegistryExportPlannedFileRole,
}

#[derive(Debug, Clone)]
struct RegistryExportExecutionPlan {
    request: RegistryExportRequest,
    snapshot: RegistryExportSnapshot,
    files: Vec<RegistryExportPlannedFile>,
    content_hash: String,
}

#[derive(Clone, Copy)]
struct RegistryExportBackendContract {
    id: &'static str,
    plan_files: fn(&RegistryExportRequest) -> Vec<RegistryExportPlannedFile>,
    execute: fn(&RegistryExportExecutionPlan) -> Result<(), Refusal>,
}

impl RegistryExportFormat {
    fn backend_contract(self) -> RegistryExportBackendContract {
        match self {
            Self::DbtSeed => RegistryExportBackendContract {
                id: self.as_str(),
                plan_files: plan_dbt_seed_files,
                execute: execute_dbt_seed_backend,
            },
            Self::SearchIndex => RegistryExportBackendContract {
                id: self.as_str(),
                plan_files: plan_search_index_files,
                execute: execute_search_index_backend,
            },
        }
    }
}

pub fn export_registry(request: RegistryExportRequest) -> Result<RegistryExportOutput, Refusal> {
    let plan = plan_registry_export(request)?;
    execute_registry_export_plan(&plan)
}

fn plan_registry_export(
    request: RegistryExportRequest,
) -> Result<RegistryExportExecutionPlan, Refusal> {
    validate_request(&request)?;
    let snapshot = build_export_snapshot(&request)?;
    let backend = request.format.backend_contract();

    Ok(RegistryExportExecutionPlan {
        content_hash: hash_backend_export(request.format, &snapshot)?,
        files: (backend.plan_files)(&request),
        request,
        snapshot,
    })
}

fn execute_registry_export_plan(
    plan: &RegistryExportExecutionPlan,
) -> Result<RegistryExportOutput, Refusal> {
    let backend = plan.request.format.backend_contract();
    (backend.execute)(plan)?;

    Ok(RegistryExportOutput {
        version: EXPORT_VERSION.to_string(),
        format: backend.id.to_string(),
        registry: plan.snapshot.registry.clone(),
        output_path: plan.request.out.display().to_string(),
        content_hash: plan.content_hash.clone(),
        normalization_spec: NORMALIZATION_SPEC_ID.to_string(),
        filters: plan.snapshot.filters.clone(),
        summary: plan.snapshot.summary(),
        files: plan
            .files
            .iter()
            .map(|file| file.path.display().to_string())
            .collect(),
    })
}

fn validate_request(request: &RegistryExportRequest) -> Result<(), Refusal> {
    if request.canonical_iri_prefix.trim().is_empty() {
        return Err(parse_refusal(
            "Registry export requires a non-empty --canonical-iri-prefix",
            json!({ "canonical_iri_prefix": request.canonical_iri_prefix }),
            "canon registry export --canonical-iri-prefix cmdrvl:",
        ));
    }

    if matches!(request.format, RegistryExportFormat::DbtSeed)
        && request.namespace.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(parse_refusal(
            "canon registry export --format dbt-seed requires --namespace",
            json!({ "format": request.format.as_str() }),
            "canon registry export --format dbt-seed --namespace <CONTEXT> --registry <DIR> --out <seed.csv>",
        ));
    }

    if matches!(request.format, RegistryExportFormat::SearchIndex)
        && (request.schema_out.is_some() || request.anti_collapse_test_out.is_some())
    {
        return Err(parse_refusal(
            "dbt scaffold outputs are only valid with --format dbt-seed",
            json!({
                "format": request.format.as_str(),
                "schema_out": request.schema_out.as_ref().map(|path| path.display().to_string()),
                "anti_collapse_test_out": request.anti_collapse_test_out.as_ref().map(|path| path.display().to_string()),
            }),
            "Remove --schema-out/--anti-collapse-test-out or use --format dbt-seed",
        ));
    }

    Ok(())
}

fn build_export_snapshot(
    request: &RegistryExportRequest,
) -> Result<RegistryExportSnapshot, Refusal> {
    let (_, registry, mapping_files) =
        load_registry_definition(&request.registry).map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;
    let package = build_export_package(request)?;

    let filters = RegistryExportFilters {
        namespace: request.namespace.clone(),
        source_files: sorted_unique(request.source_files.clone()),
        canonical_types: sorted_unique(request.canonical_types.clone()),
        rule_id_prefixes: sorted_unique(request.rule_id_prefixes.clone()),
    };
    let source_entry_count = mapping_files.iter().map(|file| file.entries.len()).sum();
    let (rows, filtered_entry_count, skipped_filter_count, skipped_shadowed_count) = collect_rows(
        &registry,
        &mapping_files,
        &filters,
        &request.canonical_iri_prefix,
    );
    let entities = collect_entities(&rows);
    let snapshot_hash = hash_snapshot(&registry, &package, &filters, &rows, &entities)?;

    Ok(RegistryExportSnapshot {
        registry,
        package,
        filters,
        source_entry_count,
        filtered_entry_count,
        skipped_filter_count,
        skipped_shadowed_count,
        rows,
        entities,
        snapshot_hash,
    })
}

fn build_export_package(request: &RegistryExportRequest) -> Result<RegistryExportPackage, Refusal> {
    let package = compile_registry_package(&request.registry).map_err(|error| {
        Refusal::bad_registry(
            &request.registry.display().to_string(),
            &format!("failed to compile registry package for export: {error}"),
        )
    })?;
    Ok(RegistryExportPackage {
        schema_version: REGISTRY_PACKAGE_SCHEMA_VERSION.to_string(),
        id: package.registry.id,
        version: package.registry.version,
        content_digest: package.content_digest,
        entry_count: package.entry_count,
        effective_mapping_count: package.effective_mapping_count,
    })
}

fn collect_rows(
    registry: &RegistryMeta,
    mapping_files: &[MappingFile],
    filters: &RegistryExportFilters,
    canonical_iri_prefix: &str,
) -> (Vec<ExportRow>, usize, usize, usize) {
    let mut rows = Vec::new();
    let mut seen_inputs = BTreeSet::new();
    let mut filtered_entry_count = 0;
    let mut skipped_filter_count = 0;
    let mut skipped_shadowed_count = 0;

    for mapping_file in mapping_files {
        let source_file = mapping_file
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        for (entry_order, entry) in mapping_file.entries.iter().enumerate() {
            if !passes_filters(
                entry.canonical_type.as_str(),
                entry.rule_id.as_str(),
                &source_file,
                filters,
            ) {
                skipped_filter_count += 1;
                continue;
            }
            filtered_entry_count += 1;
            let input = ascii_trim(&entry.input).to_string();
            if !seen_inputs.insert(input.clone()) {
                skipped_shadowed_count += 1;
                continue;
            }
            rows.push(ExportRow {
                namespace: filters.namespace.clone(),
                normalized_key: normalize_search_key(&input),
                canonical_iri: canonical_iri(&entry.canonical_id, canonical_iri_prefix),
                alias_kind: infer_alias_kind(
                    &input,
                    &entry.canonical_id,
                    &entry.canonical_type,
                    &entry.rule_id,
                ),
                match_source: infer_match_source(&entry.rule_id),
                input,
                canonical_id: entry.canonical_id.clone(),
                canonical_type: entry.canonical_type.clone(),
                rule_id: entry.rule_id.clone(),
                registry_id: registry.id.clone(),
                registry_version: registry.version.clone(),
                source_file: source_file.clone(),
                entry_order,
            });
        }
    }

    (
        rows,
        filtered_entry_count,
        skipped_filter_count,
        skipped_shadowed_count,
    )
}

fn passes_filters(
    canonical_type: &str,
    rule_id: &str,
    source_file: &str,
    filters: &RegistryExportFilters,
) -> bool {
    (filters.source_files.is_empty()
        || filters
            .source_files
            .iter()
            .any(|value| value == source_file))
        && (filters.canonical_types.is_empty()
            || filters
                .canonical_types
                .iter()
                .any(|value| value == canonical_type))
        && (filters.rule_id_prefixes.is_empty()
            || filters
                .rule_id_prefixes
                .iter()
                .any(|prefix| rule_id.starts_with(prefix)))
}

fn collect_entities(rows: &[ExportRow]) -> Vec<EntityRow> {
    let mut entities = BTreeMap::<(String, String), EntityRow>::new();
    for row in rows {
        let key = (row.canonical_type.clone(), row.canonical_id.clone());
        let rank = alias_display_rank(&row.alias_kind);
        let entry = entities.entry(key).or_insert_with(|| EntityRow {
            canonical_id: row.canonical_id.clone(),
            canonical_iri: row.canonical_iri.clone(),
            canonical_type: row.canonical_type.clone(),
            display_name: row.input.clone(),
            normalized_display_key: row.normalized_key.clone(),
            alias_count: 0,
            display_rank: rank,
        });
        entry.alias_count += 1;
        if rank < entry.display_rank {
            entry.display_name = row.input.clone();
            entry.normalized_display_key = row.normalized_key.clone();
            entry.display_rank = rank;
        }
    }

    entities.into_values().collect()
}

fn write_dbt_seed(path: &Path, plan: &RegistryExportExecutionPlan) -> Result<(), Refusal> {
    ensure_parent_dir(path)?;
    let file = File::create(path).map_err(|error| io_refusal(path, error))?;
    let mut writer = csv::Writer::from_writer(file);
    let package = &plan.snapshot.package;
    writer
        .write_record([
            "namespace",
            "source_input",
            "normalized_key",
            "canonical_id",
            "canonical_iri",
            "canonical_type",
            "alias_kind",
            "rule_id",
            "match_source",
            "registry_id",
            "registry_version",
            "registry_content_hash",
            "registry_package_id",
            "registry_package_version",
            "registry_package_digest",
            "registry_package_schema_version",
            "source_file",
            "entry_order",
        ])
        .map_err(|error| io_refusal(path, error))?;

    let content_hash = hash_rows_only(&plan.snapshot.rows)?;
    for row in &plan.snapshot.rows {
        writer
            .write_record([
                row.namespace.as_deref().unwrap_or_default(),
                &row.input,
                &row.normalized_key,
                &row.canonical_id,
                &row.canonical_iri,
                &row.canonical_type,
                &row.alias_kind,
                &row.rule_id,
                &row.match_source,
                &row.registry_id,
                &row.registry_version,
                &content_hash,
                &package.id,
                &package.version,
                &package.content_digest,
                &package.schema_version,
                &row.source_file,
                &row.entry_order.to_string(),
            ])
            .map_err(|error| io_refusal(path, error))?;
    }
    writer.flush().map_err(|error| io_refusal(path, error))?;
    Ok(())
}

fn write_dbt_schema(path: &Path, seed_path: &Path) -> Result<(), Refusal> {
    ensure_parent_dir(path)?;
    let seed_name = dbt_seed_name(seed_path);
    let body = format!(
        "version: 2\nseeds:\n  - name: {seed_name}\n    description: Canon registry export seed with immutable package provenance.\n    columns:\n      - name: namespace\n        tests:\n          - not_null\n      - name: source_input\n        tests:\n          - not_null\n      - name: normalized_key\n        tests:\n          - not_null\n      - name: canonical_id\n        tests:\n          - not_null\n      - name: canonical_iri\n        tests:\n          - not_null\n      - name: canonical_type\n        tests:\n          - not_null\n      - name: alias_kind\n        tests:\n          - not_null\n      - name: rule_id\n        tests:\n          - not_null\n      - name: match_source\n        tests:\n          - not_null\n      - name: registry_id\n        tests:\n          - not_null\n      - name: registry_version\n        tests:\n          - not_null\n      - name: registry_content_hash\n        tests:\n          - not_null\n      - name: registry_package_id\n        tests:\n          - not_null\n      - name: registry_package_version\n        tests:\n          - not_null\n      - name: registry_package_digest\n        tests:\n          - not_null\n      - name: registry_package_schema_version\n        tests:\n          - not_null\n      - name: source_file\n        tests:\n          - not_null\n      - name: entry_order\n        tests:\n          - not_null\n"
    );
    fs::write(path, body).map_err(|error| io_refusal(path, error))
}

fn write_anti_collapse_test(path: &Path, seed_path: &Path) -> Result<(), Refusal> {
    ensure_parent_dir(path)?;
    let seed_name = dbt_seed_name(seed_path);
    let body = format!(
        "select\n  namespace,\n  normalized_key,\n  count(*) as source_alias_count,\n  count(distinct canonical_id) as canonical_id_count\nfrom {{{{ ref('{seed_name}') }}}}\ngroup by 1, 2\nhaving count(distinct canonical_id) > 1\n"
    );
    fs::write(path, body).map_err(|error| io_refusal(path, error))
}

fn execute_dbt_seed_backend(plan: &RegistryExportExecutionPlan) -> Result<(), Refusal> {
    write_dbt_seed(&plan.request.out, plan)?;
    for file in &plan.files {
        match file.role {
            RegistryExportPlannedFileRole::PrimaryArtifact => {}
            RegistryExportPlannedFileRole::DbtSchema => {
                write_dbt_schema(&file.path, &plan.request.out)?;
            }
            RegistryExportPlannedFileRole::DbtAntiCollapseTest => {
                write_anti_collapse_test(&file.path, &plan.request.out)?;
            }
        }
    }
    Ok(())
}

fn execute_search_index_backend(plan: &RegistryExportExecutionPlan) -> Result<(), Refusal> {
    write_search_index(&plan.request.out, plan)
}

fn plan_dbt_seed_files(request: &RegistryExportRequest) -> Vec<RegistryExportPlannedFile> {
    let mut files = vec![RegistryExportPlannedFile {
        path: request.out.clone(),
        role: RegistryExportPlannedFileRole::PrimaryArtifact,
    }];
    if let Some(path) = &request.schema_out {
        files.push(RegistryExportPlannedFile {
            path: path.clone(),
            role: RegistryExportPlannedFileRole::DbtSchema,
        });
    }
    if let Some(path) = &request.anti_collapse_test_out {
        files.push(RegistryExportPlannedFile {
            path: path.clone(),
            role: RegistryExportPlannedFileRole::DbtAntiCollapseTest,
        });
    }
    files
}

fn plan_search_index_files(request: &RegistryExportRequest) -> Vec<RegistryExportPlannedFile> {
    vec![RegistryExportPlannedFile {
        path: request.out.clone(),
        role: RegistryExportPlannedFileRole::PrimaryArtifact,
    }]
}

fn write_search_index(path: &Path, plan: &RegistryExportExecutionPlan) -> Result<(), Refusal> {
    ensure_parent_dir(path)?;
    let mut conn = Connection::open(path).map_err(|error| io_refusal(path, error))?;
    conn.execute_batch(RESET_SEARCH_INDEX_SCHEMA)
        .map_err(|error| io_refusal(path, error))?;
    conn.execute_batch(SEARCH_INDEX_SCHEMA)
        .map_err(|error| io_refusal(path, error))?;

    let tx = conn
        .transaction()
        .map_err(|error| io_refusal(path, error))?;
    insert_search_metadata(&tx, plan, path)?;
    insert_search_capabilities(&tx, path)?;
    insert_search_scoring_spec(&tx, path)?;
    insert_entities(&tx, &plan.snapshot.entities, path)?;
    insert_aliases(&tx, &plan.snapshot.rows, path)?;
    insert_external_keys(&tx, &plan.snapshot.rows, path)?;
    tx.commit().map_err(|error| io_refusal(path, error))?;
    Ok(())
}

fn insert_search_metadata(
    conn: &Connection,
    plan: &RegistryExportExecutionPlan,
    path: &Path,
) -> Result<(), Refusal> {
    let normalization_spec = json!({
        "id": NORMALIZATION_SPEC_ID,
        "description": "ASCII uppercase, then remove every non A-Z/0-9 character",
        "steps": ["ascii_uppercase", "drop_non_ascii_alphanumeric"],
    });
    let metadata = [
        (
            "artifact_version",
            SEARCH_INDEX_ARTIFACT_VERSION.to_string(),
        ),
        ("schema_version", SEARCH_INDEX_SCHEMA_VERSION.to_string()),
        ("artifact_role", "serving_projection".to_string()),
        (
            "cache_policy",
            "standalone_export_not_internal_cache".to_string(),
        ),
        ("open_mode", "read_only_serving".to_string()),
        (
            "identity_contract",
            "exact registry snapshot; search normalization is serving-only".to_string(),
        ),
        ("registry_id", plan.snapshot.registry.id.clone()),
        ("registry_version", plan.snapshot.registry.version.clone()),
        ("registry_source", plan.snapshot.registry.source.clone()),
        (
            "registry_entry_count",
            plan.snapshot.package.entry_count.to_string(),
        ),
        (
            "registry_effective_mapping_count",
            plan.snapshot.package.effective_mapping_count.to_string(),
        ),
        (
            "registry_package_schema_version",
            plan.snapshot.package.schema_version.clone(),
        ),
        ("registry_package_id", plan.snapshot.package.id.clone()),
        (
            "registry_package_version",
            plan.snapshot.package.version.clone(),
        ),
        (
            "registry_package_digest",
            plan.snapshot.package.content_digest.clone(),
        ),
        ("format", plan.request.format.as_str().to_string()),
        ("content_hash", plan.content_hash.clone()),
        ("snapshot_hash", plan.snapshot.snapshot_hash.clone()),
        ("generated_at", "1970-01-01T00:00:00Z".to_string()),
        (
            "generation_time_policy",
            "deterministic_export_no_wall_clock".to_string(),
        ),
        (
            "source_entry_count",
            plan.snapshot.source_entry_count.to_string(),
        ),
        (
            "filtered_entry_count",
            plan.snapshot.filtered_entry_count.to_string(),
        ),
        ("exported_alias_count", plan.snapshot.rows.len().to_string()),
        (
            "exported_entity_count",
            plan.snapshot.entities.len().to_string(),
        ),
        (
            "skipped_filter_count",
            plan.snapshot.skipped_filter_count.to_string(),
        ),
        (
            "skipped_shadowed_count",
            plan.snapshot.skipped_shadowed_count.to_string(),
        ),
        ("normalization_spec_id", NORMALIZATION_SPEC_ID.to_string()),
        ("normalization_spec", normalization_spec.to_string()),
        (
            "canonical_iri_prefix",
            plan.request.canonical_iri_prefix.clone(),
        ),
        (
            "namespace",
            plan.request.namespace.clone().unwrap_or_default(),
        ),
        (
            "filters",
            serde_json::to_string(&plan.snapshot.filters).map_err(|error| {
                parse_refusal(
                    "Failed to serialize registry export filters",
                    json!({ "error": error.to_string() }),
                    "Rerun canon registry export with valid filters",
                )
            })?,
        ),
    ];
    for (key, value) in metadata {
        conn.execute(
            "INSERT INTO metadata (key, value) VALUES (?, ?)",
            params![key, value],
        )
        .map_err(|error| io_refusal(path, error))?;
    }
    Ok(())
}

fn insert_search_capabilities(conn: &Connection, path: &Path) -> Result<(), Refusal> {
    let capabilities = [
        (
            "exact_alias_lookup",
            true,
            "aliases.alias preserves exact registry aliases after ASCII-trim",
        ),
        (
            "normalized_key_search",
            true,
            "aliases.normalized_key is a serving key only and never changes canonical identity",
        ),
        (
            "fts_alias_search",
            true,
            "aliases_fts indexes alias, normalized_key, canonical_id, and canonical_iri",
        ),
        (
            "canonical_iri_projection",
            true,
            "aliases, entities, and external_keys carry canonical_iri",
        ),
        (
            "source_rule_provenance",
            true,
            "aliases carry source_file, entry_order, rule_id, match_source, and registry_version",
        ),
        (
            "registry_package_trace",
            true,
            "metadata pins registry package id, version, schema version, and digest",
        ),
        (
            "standalone_export",
            true,
            "artifact is a deployment export distinct from Canon internal registry caches",
        ),
        (
            "mutable_internal_cache",
            false,
            "search-index exports are not used as Canon derived lookup caches",
        ),
    ];

    for (capability, enabled, description) in capabilities {
        conn.execute(
            "INSERT INTO capabilities (capability, enabled, description) VALUES (?, ?, ?)",
            params![capability, if enabled { 1_i64 } else { 0_i64 }, description],
        )
        .map_err(|error| io_refusal(path, error))?;
    }
    Ok(())
}

fn insert_search_scoring_spec(conn: &Connection, path: &Path) -> Result<(), Refusal> {
    let tiers = [
        ("exact", 100, "normalized key equals query key"),
        ("prefix", 80, "normalized key starts with query key"),
        ("contains", 60, "normalized key contains query key"),
        ("reverse_contains", 50, "query key contains normalized key"),
        ("text_contains", 40, "raw alias contains raw query text"),
    ];
    for (tier, score, description) in tiers {
        conn.execute(
            "INSERT INTO scoring_tiers (tier, score, description) VALUES (?, ?, ?)",
            params![tier, score, description],
        )
        .map_err(|error| io_refusal(path, error))?;
    }

    let field_weights = [
        ("alias", 1.0_f64),
        ("canonical_id", 1.0),
        ("canonical_iri", 1.0),
    ];
    for (field, weight) in field_weights {
        conn.execute(
            "INSERT INTO field_weights (field, weight) VALUES (?, ?)",
            params![field, weight],
        )
        .map_err(|error| io_refusal(path, error))?;
    }

    let alias_kind_weights = [
        ("id", 1.10_f64),
        ("name", 1.05),
        ("short", 1.04),
        ("ticker", 1.04),
        ("variant", 1.00),
        ("alias", 1.00),
        ("key", 1.00),
    ];
    for (alias_kind, weight) in alias_kind_weights {
        conn.execute(
            "INSERT INTO alias_kind_weights (alias_kind, weight) VALUES (?, ?)",
            params![alias_kind, weight],
        )
        .map_err(|error| io_refusal(path, error))?;
    }
    Ok(())
}

fn insert_entities(conn: &Connection, entities: &[EntityRow], path: &Path) -> Result<(), Refusal> {
    for entity in entities {
        conn.execute(
            "INSERT INTO entities (canonical_id, canonical_iri, canonical_type, display_name, normalized_display_key, alias_count) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                entity.canonical_id,
                entity.canonical_iri,
                entity.canonical_type,
                entity.display_name,
                entity.normalized_display_key,
                entity.alias_count as i64,
            ],
        )
        .map_err(|error| io_refusal(path, error))?;
    }
    Ok(())
}

fn insert_aliases(conn: &Connection, rows: &[ExportRow], path: &Path) -> Result<(), Refusal> {
    for row in rows {
        conn.execute(
            "INSERT INTO aliases (alias, normalized_key, alias_kind, canonical_id, canonical_iri, canonical_type, rule_id, match_source, source_file, entry_order, registry_id, registry_version) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                row.input,
                row.normalized_key,
                row.alias_kind,
                row.canonical_id,
                row.canonical_iri,
                row.canonical_type,
                row.rule_id,
                row.match_source,
                row.source_file,
                row.entry_order as i64,
                row.registry_id,
                row.registry_version,
            ],
        )
        .map_err(|error| io_refusal(path, error))?;
    }
    conn.execute(
        "INSERT INTO aliases_fts (rowid, alias, normalized_key, canonical_id, canonical_iri) SELECT id, alias, normalized_key, canonical_id, canonical_iri FROM aliases",
        [],
    )
    .map_err(|error| io_refusal(path, error))?;
    Ok(())
}

fn insert_external_keys(conn: &Connection, rows: &[ExportRow], path: &Path) -> Result<(), Refusal> {
    let mut keys = BTreeSet::new();
    for row in rows {
        keys.insert((
            row.canonical_type.clone(),
            row.canonical_id.clone(),
            row.canonical_type.clone(),
            row.canonical_id.clone(),
            row.canonical_iri.clone(),
            "canonical_id".to_string(),
        ));
        if matches!(row.alias_kind.as_str(), "id" | "key" | "ticker") {
            keys.insert((
                row.canonical_type.clone(),
                row.canonical_id.clone(),
                row.alias_kind.clone(),
                row.input.clone(),
                row.canonical_iri.clone(),
                "alias".to_string(),
            ));
        }
    }
    for (canonical_type, canonical_id, key_namespace, key_value, canonical_iri, source) in keys {
        conn.execute(
            "INSERT INTO external_keys (canonical_type, canonical_id, key_namespace, key_value, canonical_iri, source) VALUES (?, ?, ?, ?, ?, ?)",
            params![canonical_type, canonical_id, key_namespace, key_value, canonical_iri, source],
        )
        .map_err(|error| io_refusal(path, error))?;
    }
    Ok(())
}

const RESET_SEARCH_INDEX_SCHEMA: &str = r#"
DROP TABLE IF EXISTS aliases_fts;
DROP TABLE IF EXISTS alias_kind_weights;
DROP TABLE IF EXISTS field_weights;
DROP TABLE IF EXISTS scoring_tiers;
DROP TABLE IF EXISTS capabilities;
DROP TABLE IF EXISTS external_keys;
DROP TABLE IF EXISTS aliases;
DROP TABLE IF EXISTS entities;
DROP TABLE IF EXISTS metadata;
"#;

const SEARCH_INDEX_SCHEMA: &str = r#"
CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE entities (
    canonical_id TEXT NOT NULL,
    canonical_iri TEXT NOT NULL,
    canonical_type TEXT NOT NULL,
    display_name TEXT NOT NULL,
    normalized_display_key TEXT NOT NULL,
    alias_count INTEGER NOT NULL,
    PRIMARY KEY (canonical_type, canonical_id)
);

CREATE TABLE aliases (
    id INTEGER PRIMARY KEY,
    alias TEXT NOT NULL,
    normalized_key TEXT NOT NULL,
    alias_kind TEXT NOT NULL,
    canonical_id TEXT NOT NULL,
    canonical_iri TEXT NOT NULL,
    canonical_type TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    match_source TEXT NOT NULL,
    source_file TEXT NOT NULL,
    entry_order INTEGER NOT NULL,
    registry_id TEXT NOT NULL,
    registry_version TEXT NOT NULL
);

CREATE INDEX idx_aliases_normalized_key ON aliases(normalized_key);
CREATE INDEX idx_aliases_canonical_iri ON aliases(canonical_iri);
CREATE INDEX idx_aliases_kind_key ON aliases(alias_kind, normalized_key);

CREATE TABLE external_keys (
    canonical_type TEXT NOT NULL,
    canonical_id TEXT NOT NULL,
    key_namespace TEXT NOT NULL,
    key_value TEXT NOT NULL,
    canonical_iri TEXT NOT NULL,
    source TEXT NOT NULL,
    PRIMARY KEY (canonical_type, canonical_id, key_namespace, key_value)
);

CREATE INDEX idx_external_keys_lookup ON external_keys(key_namespace, key_value);

CREATE TABLE capabilities (
    capability TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    description TEXT NOT NULL
);

CREATE TABLE scoring_tiers (
    tier TEXT PRIMARY KEY,
    score INTEGER NOT NULL,
    description TEXT NOT NULL
);

CREATE TABLE field_weights (
    field TEXT PRIMARY KEY,
    weight REAL NOT NULL
);

CREATE TABLE alias_kind_weights (
    alias_kind TEXT PRIMARY KEY,
    weight REAL NOT NULL
);

CREATE VIRTUAL TABLE aliases_fts
USING fts5(alias, normalized_key, canonical_id, canonical_iri, content='aliases', content_rowid='id');
"#;

fn hash_backend_export(
    format: RegistryExportFormat,
    snapshot: &RegistryExportSnapshot,
) -> Result<String, Refusal> {
    let value = json!({
        "format": format.as_str(),
        "registry": registry_hash_identity(&snapshot.registry),
        "package": &snapshot.package,
        "filters": &snapshot.filters,
        "rows": &snapshot.rows,
        "entities": &snapshot.entities,
    });
    hash_json_value(&value)
}

fn hash_snapshot(
    registry: &RegistryMeta,
    package: &RegistryExportPackage,
    filters: &RegistryExportFilters,
    rows: &[ExportRow],
    entities: &[EntityRow],
) -> Result<String, Refusal> {
    let value = json!({
        "registry": registry_hash_identity(registry),
        "package": package,
        "filters": filters,
        "rows": rows,
        "entities": entities,
    });
    hash_json_value(&value)
}

fn hash_rows_only(rows: &[ExportRow]) -> Result<String, Refusal> {
    hash_json_value(&json!({ "rows": rows }))
}

fn registry_hash_identity(registry: &RegistryMeta) -> serde_json::Value {
    json!({
        "id": registry.id,
        "version": registry.version,
    })
}

fn hash_json_value(value: &serde_json::Value) -> Result<String, Refusal> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        parse_refusal(
            "Failed to serialize registry export hash input",
            json!({ "error": error.to_string() }),
            "Rerun canon registry export after validating registry JSON",
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn normalize_search_key(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_uppercase())
            } else {
                None
            }
        })
        .collect()
}

fn infer_alias_kind(
    input: &str,
    canonical_id: &str,
    canonical_type: &str,
    rule_id: &str,
) -> String {
    let rule = rule_id.to_ascii_uppercase();
    let canonical_type_upper = canonical_type.to_ascii_uppercase();
    if normalize_search_key(input) == normalize_search_key(canonical_id) {
        return "id".to_string();
    }
    if rule.contains("TICKER") || canonical_type_upper == "TICKER" {
        return "ticker".to_string();
    }
    if rule.contains("SHORT") {
        return "short".to_string();
    }
    if rule.contains("NAME") || rule.contains("TRUST") {
        return "name".to_string();
    }
    if rule.contains("VARIANT") || rule.contains("ALIAS") || rule.contains("BRAND") {
        return "variant".to_string();
    }
    if looks_like_key(input) {
        return "key".to_string();
    }
    "alias".to_string()
}

fn infer_match_source(rule_id: &str) -> String {
    let rule = rule_id.to_ascii_uppercase();
    if rule.contains("BRAND") {
        "registry_brand".to_string()
    } else if rule.contains("CANON") {
        "canon".to_string()
    } else {
        "registry_exact".to_string()
    }
}

fn looks_like_key(value: &str) -> bool {
    let value = ascii_trim(value);
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ':')
        && value.chars().any(|ch| ch.is_ascii_digit())
}

fn alias_display_rank(alias_kind: &str) -> u8 {
    match alias_kind {
        "name" => 0,
        "id" => 1,
        "short" | "ticker" => 2,
        "variant" | "alias" => 3,
        "key" => 4,
        _ => 5,
    }
}

fn canonical_iri(canonical_id: &str, prefix: &str) -> String {
    if canonical_id.starts_with(prefix)
        || canonical_id.starts_with("cmdrvl:")
        || canonical_id.starts_with("urn:")
        || canonical_id.contains("://")
    {
        canonical_id.to_string()
    } else {
        format!("{prefix}{canonical_id}")
    }
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

fn dbt_seed_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("canon_registry_seed")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn ensure_parent_dir(path: &Path) -> Result<(), Refusal> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| io_refusal(parent, error))?;
    }
    Ok(())
}

fn io_refusal(path: &Path, error: impl std::error::Error) -> Refusal {
    Refusal {
        code: RefusalCode::EIo,
        message: format!("Registry export failed at '{}': {}", path.display(), error),
        detail: json!({
            "path": path.display().to_string(),
            "error": error.to_string(),
        }),
        next_command: Some(
            "Check output paths and permissions, then rerun canon registry export".to_string(),
        ),
    }
}

fn normalize_export_temporal_scope(
    valid_at: &str,
    known_as_of: &str,
) -> Result<ExportTemporalScope, Refusal> {
    let valid_at = normalized_non_empty(valid_at, "valid_at")?;
    let known_as_of = normalized_non_empty(known_as_of, "known_as_of")?;
    let valid_timeless = valid_at.eq_ignore_ascii_case("timeless");
    let known_timeless = known_as_of.eq_ignore_ascii_case("timeless");
    match (valid_timeless, known_timeless) {
        (true, true) => Ok(ExportTemporalScope {
            mode: "timeless",
            valid_at: "timeless".to_string(),
            known_as_of: "timeless".to_string(),
            precision: "not_applicable",
        }),
        (true, false) | (false, true) => Err(parse_refusal(
            "temporal export timeless mode must set both valid_at and known_as_of to timeless",
            json!({
                "valid_at": valid_at,
                "known_as_of": known_as_of,
            }),
            "Use valid_at=timeless and known_as_of=timeless, or provide two RFC3339 instants",
        )),
        (false, false) => Ok(ExportTemporalScope {
            mode: "as_of",
            valid_at: canonical_export_instant(&valid_at, "valid_at")?,
            known_as_of: canonical_export_instant(&known_as_of, "known_as_of")?,
            precision: "instant",
        }),
    }
}

fn canonical_export_instant(value: &str, field: &str) -> Result<String, Refusal> {
    if is_date_only(value) {
        return Err(parse_refusal(
            "temporal export instants must not fabricate timestamps from date-only disclosures",
            json!({ "field": field, "value": value }),
            "Provide an RFC3339 instant with an explicit timezone, or use timeless mode",
        ));
    }
    let parsed = DateTime::parse_from_rfc3339(value).map_err(|error| {
        parse_refusal(
            "temporal export instants must be RFC3339 with an explicit timezone",
            json!({ "field": field, "value": value, "error": error.to_string() }),
            "Provide an RFC3339 instant such as 2026-07-10T12:00:00Z",
        )
    })?;
    Ok(parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn normalized_blake3_digest(value: &str, field: &str) -> Result<String, Refusal> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(parse_refusal(
            "temporal export digest must use blake3:<hex> encoding",
            json!({ "field": field, "value": value }),
            "Pass the compiled temporal snapshot digest from the temporal compile artifact",
        ));
    };
    if hex.len() == 64 && hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Ok(format!("blake3:{}", hex.to_ascii_lowercase()));
    }
    Err(parse_refusal(
        "temporal export digest must contain a 64-character hex digest",
        json!({ "field": field, "value": value }),
        "Pass the compiled temporal snapshot digest from the temporal compile artifact",
    ))
}

fn normalized_non_empty(value: &str, field: &str) -> Result<String, Refusal> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        return Err(parse_refusal(
            "temporal export metadata field is required",
            json!({ "field": field }),
            "Provide all temporal projection metadata fields",
        ));
    }
    Ok(normalized)
}

fn is_date_only(value: &str) -> bool {
    value.len() == "YYYY-MM-DD".len()
        && value.chars().enumerate().all(|(index, character)| {
            if matches!(index, 4 | 7) {
                character == '-'
            } else {
                character.is_ascii_digit()
            }
        })
}

fn parse_refusal(message: &str, detail: serde_json::Value, next_command: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: message.to_string(),
        detail,
        next_command: Some(next_command.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_search_key;

    #[test]
    fn search_key_is_ascii_upper_alnum_only() {
        assert_eq!(
            normalize_search_key("Wells Fargo 2020-C58"),
            "WELLSFARGO2020C58"
        );
        assert_eq!(normalize_search_key("wfcm-2020_c58"), "WFCM2020C58");
        assert_eq!(normalize_search_key(" café "), "CAF");
    }
}
