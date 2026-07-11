use crate::{
    Refusal,
    entity::runtime::types::{
        AnchorValue, CannotLinkFact, PendingClusterRecord, RowPair, TrustedAnchorRecord,
    },
    registry::package::{
        RegistryPackage, RegistryPackageError, RegistryPackageFindingSeverity,
        RegistryPackageVerificationFinding, compile_registry_package, verify_registry_package,
    },
    strategy_registry::{
        StrategyAttestationGrade, StrategyEntryKey, StrategyEntryStatus, StrategyRegistryEntry,
        StrategySchemaShape,
    },
};
use rusqlite::{Connection, Error as SqliteError};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryLintProfile {
    Standard,
    Org,
    Strategy,
    Package,
    Auto,
}

impl RegistryLintProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Org => "org",
            Self::Strategy => "strategy",
            Self::Package => "package",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryLintRegistry {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lookup_snapshot_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escrow_snapshot_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryLintSeverity {
    Info,
    Warning,
    Error,
}

impl RegistryLintSeverity {
    fn as_count_key(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryLintFinding {
    pub severity: RegistryLintSeverity,
    pub category: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub detail: Value,
    pub next_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryLintSummary {
    pub total_findings: usize,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub checked_profiles: Vec<String>,
    pub by_category: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryLintOutput {
    pub version: String,
    pub requested_profile: String,
    pub profile: String,
    pub registry: RegistryLintRegistry,
    pub summary: RegistryLintSummary,
    pub findings: Vec<RegistryLintFinding>,
    pub next_command: String,
}

impl RegistryLintOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{}@{} lint {} | findings={} errors={} warnings={} info={}",
            self.registry.id.as_deref().unwrap_or("<unknown>"),
            self.registry.version.as_deref().unwrap_or("<unknown>"),
            self.profile,
            self.summary.total_findings,
            self.summary.errors,
            self.summary.warnings,
            self.summary.info,
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawRegistryJson {
    id: Option<String>,
    version: Option<String>,
    entry_count: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawMappingEntry {
    input: Option<String>,
    canonical_id: Option<String>,
    canonical_type: Option<String>,
    rule_id: Option<String>,
}

#[derive(Debug, Clone)]
struct MappingRecord {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
    source_file: String,
    entry_order: usize,
}

#[derive(Debug)]
struct LintContext {
    registry_dir: PathBuf,
    registry: RegistryLintRegistry,
    findings: Vec<RegistryLintFinding>,
}

impl LintContext {
    fn new(registry_dir: &Path, registry: RegistryLintRegistry) -> Self {
        Self {
            registry_dir: registry_dir.to_path_buf(),
            registry,
            findings: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finding(
        &mut self,
        severity: RegistryLintSeverity,
        category: &str,
        code: &str,
        message: impl Into<String>,
        location: FindingLocation,
        detail: Value,
        next_command: impl Into<String>,
    ) {
        self.findings.push(RegistryLintFinding {
            severity,
            category: category.to_string(),
            code: code.to_string(),
            message: message.into(),
            path: location.path.map(|path| path.display().to_string()),
            line: location.line,
            detail,
            next_command: next_command.into(),
        });
    }
}

#[derive(Debug, Default)]
struct FindingLocation {
    path: Option<PathBuf>,
    line: Option<usize>,
}

impl FindingLocation {
    fn path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            line: None,
        }
    }

    fn line(path: impl Into<PathBuf>, line: usize) -> Self {
        Self {
            path: Some(path.into()),
            line: Some(line),
        }
    }
}

pub fn lint(
    registry_dir: &Path,
    requested_profile: RegistryLintProfile,
) -> Result<RegistryLintOutput, Refusal> {
    if !registry_dir.is_dir() {
        return Err(Refusal::bad_registry(
            &registry_dir.display().to_string(),
            "registry directory not found",
        ));
    }

    let (registry, registry_findings) = read_registry_metadata(registry_dir);
    let profile = resolve_profile(registry_dir, requested_profile);
    let mut context = LintContext::new(registry_dir, registry);
    context.findings.extend(registry_findings);

    match profile {
        RegistryLintProfile::Standard => lint_standard(&mut context),
        RegistryLintProfile::Org => lint_org(&mut context),
        RegistryLintProfile::Strategy => lint_strategy(&mut context),
        RegistryLintProfile::Package => lint_package(&mut context),
        RegistryLintProfile::Auto => unreachable!("auto is resolved before lint dispatch"),
    }

    context.findings.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.message.cmp(&right.message))
    });

    let summary = summarize_findings(&context.findings, profile);
    let next_command = next_command_for_summary(&summary).to_string();

    Ok(RegistryLintOutput {
        version: "canon_registry_lint.v0".to_string(),
        requested_profile: requested_profile.as_str().to_string(),
        profile: profile.as_str().to_string(),
        registry: context.registry,
        summary,
        findings: context.findings,
        next_command,
    })
}

pub fn lint_registry_package(
    registry_dir: &Path,
    package: &RegistryPackage,
) -> Result<RegistryLintOutput, RegistryPackageError> {
    let (registry, registry_findings) = read_registry_metadata(registry_dir);
    let mut context = LintContext::new(registry_dir, registry);
    context.findings.extend(registry_findings);
    append_package_verify_findings(&mut context, package)?;
    context.findings.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.message.cmp(&right.message))
    });

    let summary = summarize_findings(&context.findings, RegistryLintProfile::Package);
    let next_command = next_command_for_summary(&summary).to_string();

    Ok(RegistryLintOutput {
        version: "canon_registry_lint.v0".to_string(),
        requested_profile: RegistryLintProfile::Package.as_str().to_string(),
        profile: RegistryLintProfile::Package.as_str().to_string(),
        registry: context.registry,
        summary,
        findings: context.findings,
        next_command,
    })
}

fn read_registry_metadata(registry_dir: &Path) -> (RegistryLintRegistry, Vec<RegistryLintFinding>) {
    let source = registry_dir.display().to_string();
    let mut findings = Vec::new();
    let registry_path = registry_dir.join("registry.json");
    let bytes = match fs::read(&registry_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            findings.push(standalone_finding(
                RegistryLintSeverity::Error,
                "registry_json",
                "registry_json_unreadable",
                format!("Failed to read registry.json: {error}"),
                FindingLocation::path(&registry_path),
                json!({ "error": error.to_string() }),
                "Restore registry.json, then rerun canon registry lint",
            ));
            return (
                RegistryLintRegistry {
                    source,
                    id: None,
                    version: None,
                    entry_count: None,
                    lookup_snapshot_hash: None,
                    escrow_snapshot_hash: None,
                },
                findings,
            );
        }
    };

    let raw: RawRegistryJson = match serde_json::from_slice(&bytes) {
        Ok(raw) => raw,
        Err(error) => {
            findings.push(standalone_finding(
                RegistryLintSeverity::Error,
                "registry_json",
                "registry_json_malformed",
                format!("Failed to parse registry.json: {error}"),
                FindingLocation::path(&registry_path),
                json!({ "error": error.to_string() }),
                "Fix registry.json syntax and required fields, then rerun canon registry lint",
            ));
            return (
                RegistryLintRegistry {
                    source,
                    id: None,
                    version: None,
                    entry_count: None,
                    lookup_snapshot_hash: None,
                    escrow_snapshot_hash: None,
                },
                findings,
            );
        }
    };

    if raw.id.as_deref().is_none_or(|id| id.trim().is_empty()) {
        findings.push(standalone_finding(
            RegistryLintSeverity::Error,
            "registry_json",
            "registry_id_empty",
            "registry.json must contain a non-empty id",
            FindingLocation::path(&registry_path),
            Value::Null,
            "Set registry.json id, then rerun canon registry lint",
        ));
    }
    if raw
        .version
        .as_deref()
        .is_none_or(|version| version.trim().is_empty())
    {
        findings.push(standalone_finding(
            RegistryLintSeverity::Error,
            "registry_json",
            "registry_version_empty",
            "registry.json must contain a non-empty version",
            FindingLocation::path(&registry_path),
            Value::Null,
            "Set registry.json version, then rerun canon registry lint",
        ));
    }
    if raw.entry_count.is_none() {
        findings.push(standalone_finding(
            RegistryLintSeverity::Error,
            "registry_json",
            "registry_entry_count_missing",
            "registry.json must contain entry_count",
            FindingLocation::path(&registry_path),
            Value::Null,
            "Set registry.json entry_count, then rerun canon registry lint",
        ));
    }

    (
        RegistryLintRegistry {
            source,
            id: raw.id.map(|id| id.trim().to_string()),
            version: raw.version.map(|version| version.trim().to_string()),
            entry_count: raw.entry_count,
            lookup_snapshot_hash: None,
            escrow_snapshot_hash: None,
        },
        findings,
    )
}

fn resolve_profile(registry_dir: &Path, requested: RegistryLintProfile) -> RegistryLintProfile {
    match requested {
        RegistryLintProfile::Auto if registry_dir.join("_strategy").exists() => {
            RegistryLintProfile::Strategy
        }
        RegistryLintProfile::Auto
            if registry_dir.join("_anchors").exists() || registry_dir.join("_escrow").exists() =>
        {
            RegistryLintProfile::Org
        }
        RegistryLintProfile::Auto => RegistryLintProfile::Standard,
        explicit => explicit,
    }
}

fn lint_standard(context: &mut LintContext) {
    let registry_dir = context.registry_dir.clone();
    let mapping_files = discover_top_level_json_files(&registry_dir, true, context);
    let records = load_mapping_records(context, &mapping_files, "standard mapping");
    check_entry_count(context, records.len(), "standard_entries");
    check_duplicate_inputs(context, &records);
    check_index_status(context, &mapping_files);
}

fn lint_package(context: &mut LintContext) {
    match compile_registry_package(&context.registry_dir) {
        Ok(package) => {
            if let Err(error) = append_package_verify_findings(context, &package) {
                context.finding(
                    RegistryLintSeverity::Error,
                    "registry_package",
                    "package_verify_failed",
                    format!("Registry package verification failed: {error}"),
                    FindingLocation::default(),
                    json!({
                        "error_kind": format!("{:?}", error.kind),
                        "error": error.message,
                    }),
                    "Repair registry package material, then rerun canon registry lint",
                );
            }
        }
        Err(error) => context.finding(
            RegistryLintSeverity::Error,
            "registry_package",
            "package_compile_failed",
            format!("Registry package could not be compiled for lint: {error}"),
            FindingLocation::default(),
            json!({
                "error_kind": format!("{:?}", error.kind),
                "error": error.message,
            }),
            "Repair registry package material, then rerun canon registry lint",
        ),
    }
}

fn append_package_verify_findings(
    context: &mut LintContext,
    package: &RegistryPackage,
) -> Result<(), RegistryPackageError> {
    let report = verify_registry_package(&context.registry_dir, package)?;
    context
        .findings
        .extend(report.findings.into_iter().map(package_finding_to_lint));
    Ok(())
}

fn package_finding_to_lint(finding: RegistryPackageVerificationFinding) -> RegistryLintFinding {
    let severity = match finding.severity {
        RegistryPackageFindingSeverity::Error => RegistryLintSeverity::Error,
        RegistryPackageFindingSeverity::Warning => RegistryLintSeverity::Warning,
        RegistryPackageFindingSeverity::Info => RegistryLintSeverity::Info,
    };
    let next_command = match severity {
        RegistryLintSeverity::Error => {
            "Repair or rebuild the registry package, then rerun canon registry lint"
        }
        RegistryLintSeverity::Warning => "Review package warning findings before promotion",
        RegistryLintSeverity::Info => "No package lint action required",
    };
    RegistryLintFinding {
        severity,
        category: "registry_package".to_string(),
        code: finding.code,
        message: finding.message,
        path: finding.path,
        line: None,
        detail: finding.detail,
        next_command: next_command.to_string(),
    }
}

fn lint_strategy(context: &mut LintContext) {
    let strategy_dir = context.registry_dir.join("_strategy");
    if !strategy_dir.exists() {
        check_entry_count(context, 0, "strategy_entries");
        context.finding(
            RegistryLintSeverity::Warning,
            "strategy_entries",
            "strategy_directory_missing",
            "Strategy profile selected but _strategy directory is absent",
            FindingLocation::path(&strategy_dir),
            Value::Null,
            "Create _strategy entries with canon strategy register or use --profile standard",
        );
        return;
    }
    if !strategy_dir.is_dir() {
        context.finding(
            RegistryLintSeverity::Error,
            "strategy_entries",
            "strategy_path_not_directory",
            "_strategy exists but is not a directory",
            FindingLocation::path(&strategy_dir),
            Value::Null,
            "Replace _strategy with a directory, then rerun canon registry lint",
        );
        return;
    }

    let strategy_files = discover_json_files_in_dir(context, &strategy_dir, "strategy_entries");
    let records = load_strategy_records(context, &strategy_files);
    check_entry_count(context, records.len(), "strategy_entries");
    check_duplicate_strategy_keys(context, &records);
}

fn lint_org(context: &mut LintContext) {
    let registry_dir = context.registry_dir.clone();
    let alias_files = discover_top_level_json_files(&registry_dir, true, context);
    let aliases = load_alias_records(context, &alias_files);
    check_entry_count(context, aliases.len(), "org_aliases");
    check_alias_conflicts(context, &aliases);

    let anchor_files = discover_anchor_files(context);
    let anchors = load_anchor_records(context, &anchor_files);
    check_anchor_conflicts(context, &anchors);

    let pending = load_pending_records(context);
    check_pending_conflicts(context, &pending);

    let cannot_link = load_cannot_link_records(context);
    check_cannot_link_conflicts(context, &cannot_link);

    set_org_snapshot_hashes(context, &alias_files, &anchor_files);
}

fn check_entry_count(context: &mut LintContext, actual: usize, category: &str) {
    if let Some(expected) = context.registry.entry_count
        && expected != actual
    {
        context.finding(
            RegistryLintSeverity::Warning,
            category,
            "entry_count_stale",
            format!(
                "registry.json entry_count ({expected}) differs from actual lint count ({actual})"
            ),
            FindingLocation::path(context.registry_dir.join("registry.json")),
            json!({
                "registry_entry_count": expected,
                "actual_entry_count": actual,
            }),
            format!("Update registry.json entry_count to {actual}, then rerun canon registry lint"),
        );
    }
}

fn discover_top_level_json_files(
    registry_dir: &Path,
    exclude_build_artifacts: bool,
    context: &mut LintContext,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = match fs::read_dir(registry_dir) {
        Ok(entries) => entries,
        Err(error) => {
            context.finding(
                RegistryLintSeverity::Error,
                "registry_directory",
                "registry_directory_unreadable",
                format!("Failed to read registry directory: {error}"),
                FindingLocation::path(registry_dir),
                json!({ "error": error.to_string() }),
                "Fix registry directory permissions, then rerun canon registry lint",
            );
            return files;
        }
    };

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !path.is_file() || path.extension() != Some("json".as_ref()) {
            continue;
        }
        let file_name = path.file_name();
        if file_name == Some("registry.json".as_ref())
            || (exclude_build_artifacts && file_name == Some("_build.json".as_ref()))
        {
            continue;
        }
        files.push(path);
    }
    files.sort();
    files
}

fn discover_json_files_in_dir(
    context: &mut LintContext,
    dir: &Path,
    category: &str,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            context.finding(
                RegistryLintSeverity::Error,
                category,
                "directory_unreadable",
                format!("Failed to read directory '{}': {error}", dir.display()),
                FindingLocation::path(dir),
                json!({ "error": error.to_string() }),
                "Fix sidecar directory permissions, then rerun canon registry lint",
            );
            return files;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension() == Some("json".as_ref()) {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn load_mapping_records(
    context: &mut LintContext,
    files: &[PathBuf],
    label: &str,
) -> Vec<MappingRecord> {
    let mut records = Vec::new();
    for path in files {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                context.finding(
                    RegistryLintSeverity::Error,
                    "mapping_parse",
                    "mapping_file_unreadable",
                    format!("Failed to read {label} file '{}': {error}", path.display()),
                    FindingLocation::path(path),
                    json!({ "error": error.to_string() }),
                    "Fix file permissions, then rerun canon registry lint",
                );
                continue;
            }
        };
        let entries = match serde_json::from_slice::<Vec<RawMappingEntry>>(&bytes) {
            Ok(entries) => entries,
            Err(error) => {
                context.finding(
                    RegistryLintSeverity::Error,
                    "mapping_parse",
                    "mapping_file_malformed",
                    format!("Failed to parse {label} file '{}': {error}", path.display()),
                    FindingLocation::path(path),
                    json!({ "error": error.to_string() }),
                    "Fix mapping JSON syntax, then rerun canon registry lint",
                );
                continue;
            }
        };

        let source_file = file_name(path);
        for (entry_order, entry) in entries.into_iter().enumerate() {
            validate_mapping_entry(context, path, entry_order, &entry);
            if let Some(record) = mapping_record_from_raw(entry, &source_file, entry_order) {
                records.push(record);
            }
        }
    }
    records
}

fn validate_mapping_entry(
    context: &mut LintContext,
    path: &Path,
    entry_order: usize,
    entry: &RawMappingEntry,
) {
    for (field, value) in [
        ("input", entry.input.as_deref()),
        ("canonical_id", entry.canonical_id.as_deref()),
        ("canonical_type", entry.canonical_type.as_deref()),
        ("rule_id", entry.rule_id.as_deref()),
    ] {
        if value.is_none_or(|value| value.trim().is_empty()) {
            context.finding(
                RegistryLintSeverity::Error,
                "required_fields",
                "mapping_required_field_empty",
                format!("Mapping entry {entry_order} has empty required field '{field}'"),
                FindingLocation::path(path),
                json!({
                    "entry_order": entry_order,
                    "field": field,
                }),
                "Populate required mapping fields, then rerun canon registry lint",
            );
        }
    }
}

fn mapping_record_from_raw(
    entry: RawMappingEntry,
    source_file: &str,
    entry_order: usize,
) -> Option<MappingRecord> {
    let input = non_empty(entry.input)?;
    let canonical_id = non_empty(entry.canonical_id)?;
    let canonical_type = non_empty(entry.canonical_type)?;
    let rule_id = non_empty(entry.rule_id)?;
    Some(MappingRecord {
        input,
        canonical_id,
        canonical_type,
        rule_id,
        source_file: source_file.to_string(),
        entry_order,
    })
}

fn check_duplicate_inputs(context: &mut LintContext, records: &[MappingRecord]) {
    let mut first_by_input = BTreeMap::<String, &MappingRecord>::new();
    for record in records {
        if let Some(first) = first_by_input.get(&record.input) {
            let exact_duplicate = first.canonical_id == record.canonical_id
                && first.canonical_type == record.canonical_type
                && first.rule_id == record.rule_id;
            context.finding(
                RegistryLintSeverity::Warning,
                "duplicates",
                if exact_duplicate {
                    "duplicate_exact_input"
                } else {
                    "shadowed_input"
                },
                format!(
                    "Input '{}' is shadowed by earlier mapping file precedence",
                    record.input
                ),
                FindingLocation::default(),
                json!({
                    "input": record.input,
                    "first": mapping_record_detail(first),
                    "shadowed": mapping_record_detail(record),
                }),
                "Remove or reorder duplicate mapping entries, then rerun canon registry lint",
            );
        } else {
            first_by_input.insert(record.input.clone(), record);
        }
    }
}

fn check_index_status(context: &mut LintContext, mapping_files: &[PathBuf]) {
    let db_path = context.registry_dir.join("_index.sqlite");
    if !db_path.exists() {
        context.finding(
            RegistryLintSeverity::Info,
            "index",
            "index_missing",
            "SQLite lookup index is absent and will be rebuilt on next registry load",
            FindingLocation::path(&db_path),
            Value::Null,
            "Run a canon resolve command or rebuild the registry index before production use",
        );
        return;
    }

    let Some(version) = context.registry.version.as_deref() else {
        return;
    };
    let conn = match Connection::open(&db_path) {
        Ok(conn) => conn,
        Err(error) => {
            context.finding(
                RegistryLintSeverity::Warning,
                "index",
                "index_corrupt",
                format!("SQLite lookup index cannot be opened: {error}"),
                FindingLocation::path(&db_path),
                json!({ "error": error.to_string() }),
                "Delete _index.sqlite or run canon to rebuild it",
            );
            return;
        }
    };

    let stored_version = read_index_metadata_value(&conn, "version");
    match stored_version {
        Ok(stored) if stored != version => context.finding(
            RegistryLintSeverity::Info,
            "index",
            "index_version_stale",
            "SQLite lookup index version differs from registry.json",
            FindingLocation::path(&db_path),
            json!({ "index_version": stored, "registry_version": version }),
            "Run canon once to rebuild _index.sqlite for the current registry version",
        ),
        Err(error) => context.finding(
            RegistryLintSeverity::Info,
            "index",
            "index_metadata_missing",
            format!("SQLite lookup index metadata cannot be read: {error}"),
            FindingLocation::path(&db_path),
            json!({ "error": error.to_string() }),
            "Run canon once to rebuild _index.sqlite metadata",
        ),
        _ => {}
    }

    let registry_json_path = context.registry_dir.join("registry.json");
    let current_max_mtime = max_json_mtime(
        mapping_files
            .iter()
            .chain(std::iter::once(&registry_json_path)),
    );
    if let Some(current_max_mtime) = current_max_mtime {
        let stored_max_mtime = read_index_metadata_value(&conn, "max_mtime");
        if let Ok(stored) = stored_max_mtime
            && stored
                .parse::<u64>()
                .ok()
                .is_some_and(|mtime| current_max_mtime > mtime)
        {
            context.finding(
                RegistryLintSeverity::Info,
                "index",
                "index_mtime_stale",
                "SQLite lookup index is older than registry JSON inputs",
                FindingLocation::path(&db_path),
                json!({ "current_max_mtime": current_max_mtime, "index_max_mtime": stored }),
                "Run canon once to rebuild _index.sqlite for the current mapping files",
            );
        }
    }
}

fn load_strategy_records(
    context: &mut LintContext,
    files: &[PathBuf],
) -> Vec<(StrategyRegistryEntry, String, usize)> {
    let mut records = Vec::new();
    for path in files {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                context.finding(
                    RegistryLintSeverity::Error,
                    "strategy_parse",
                    "strategy_file_unreadable",
                    format!("Failed to read strategy file '{}': {error}", path.display()),
                    FindingLocation::path(path),
                    json!({ "error": error.to_string() }),
                    "Fix strategy file permissions, then rerun canon registry lint",
                );
                continue;
            }
        };
        let entries = match serde_json::from_slice::<Vec<StrategyRegistryEntry>>(&bytes) {
            Ok(entries) => entries,
            Err(error) => {
                context.finding(
                    RegistryLintSeverity::Error,
                    "strategy_parse",
                    "strategy_file_malformed",
                    format!(
                        "Failed to parse strategy file '{}': {error}",
                        path.display()
                    ),
                    FindingLocation::path(path),
                    json!({ "error": error.to_string() }),
                    "Fix strategy entry JSON, then rerun canon registry lint",
                );
                continue;
            }
        };
        let source_file = file_name(path);
        for (entry_order, entry) in entries.into_iter().enumerate() {
            check_strategy_entry(context, path, entry_order, &entry);
            records.push((entry, source_file.clone(), entry_order));
        }
    }
    records
}

fn check_strategy_entry(
    context: &mut LintContext,
    path: &Path,
    entry_order: usize,
    entry: &StrategyRegistryEntry,
) {
    for (field, value) in [
        ("entry_schema_version", entry.entry_schema_version.as_str()),
        ("skill_hash", entry.skill_hash.as_str()),
        ("script.id", entry.script.id.as_str()),
        ("script.path", entry.script.path.as_str()),
        ("script.language", entry.script.language.as_str()),
        ("script.content_hash", entry.script.content_hash.as_str()),
        ("rule_id", entry.rule_id.as_str()),
    ] {
        if value.trim().is_empty() {
            context.finding(
                RegistryLintSeverity::Error,
                "strategy_metadata",
                "strategy_required_field_empty",
                format!("Strategy entry {entry_order} has empty required field '{field}'"),
                FindingLocation::path(path),
                json!({ "entry_order": entry_order, "field": field }),
                "Populate strategy metadata, then rerun canon registry lint",
            );
        }
    }

    if entry.entry_schema_version != "canon_strategy_entry.v1" {
        context.finding(
            RegistryLintSeverity::Error,
            "strategy_metadata",
            "strategy_schema_version_invalid",
            format!("Strategy entry {entry_order} has invalid entry_schema_version"),
            FindingLocation::path(path),
            json!({
                "entry_order": entry_order,
                "entry_schema_version": entry.entry_schema_version,
            }),
            "Rewrite the strategy entry with canon strategy register/update/promote/deprecate",
        );
    }

    if entry.key.skill_hash() != entry.skill_hash {
        context.finding(
            RegistryLintSeverity::Error,
            "strategy_metadata",
            "strategy_key_skill_hash_mismatch",
            format!("Strategy entry {entry_order} key skill_hash does not match entry skill_hash"),
            FindingLocation::path(path),
            json!({
                "entry_order": entry_order,
                "key_skill_hash": entry.key.skill_hash(),
                "skill_hash": entry.skill_hash,
            }),
            "Rewrite the strategy entry with canon strategy register/update/promote/deprecate",
        );
    }

    match &entry.key {
        StrategyEntryKey::Schema {
            schema_fingerprint, ..
        } => {
            let Some(schema) = entry.schema.as_ref() else {
                context.finding(
                    RegistryLintSeverity::Error,
                    "strategy_metadata",
                    "strategy_schema_missing",
                    format!("Strategy entry {entry_order} has a schema key but no schema"),
                    FindingLocation::path(path),
                    json!({ "entry_order": entry_order }),
                    "Rewrite the schema-keyed entry with canon strategy register",
                );
                return;
            };
            let actual_fingerprint = hash_json(schema);
            if actual_fingerprint != *schema_fingerprint {
                context.finding(
                    RegistryLintSeverity::Error,
                    "strategy_fingerprint",
                    "schema_fingerprint_mismatch",
                    format!(
                        "Strategy entry {entry_order} schema_fingerprint does not match schema bytes"
                    ),
                    FindingLocation::path(path),
                    json!({
                        "entry_order": entry_order,
                        "expected": schema_fingerprint,
                        "actual": actual_fingerprint,
                    }),
                    "Regenerate the strategy entry with canon strategy register",
                );
            }
        }
        StrategyEntryKey::Task { task, .. } => {
            if task.trim().is_empty() {
                context.finding(
                    RegistryLintSeverity::Error,
                    "strategy_metadata",
                    "strategy_task_empty",
                    format!("Strategy entry {entry_order} has an empty task key"),
                    FindingLocation::path(path),
                    json!({ "entry_order": entry_order }),
                    "Register the task entry again with a non-empty --task",
                );
            }
        }
    }

    match entry.grade {
        StrategyAttestationGrade::ProofAttested => {
            let Some(proofs) = entry.proofs.as_ref() else {
                context.finding(
                    RegistryLintSeverity::Error,
                    "strategy_proofs",
                    "strategy_proof_incomplete",
                    format!("Strategy entry {entry_order} is proof-attested but has no proofs"),
                    FindingLocation::path(path),
                    json!({ "entry_order": entry_order }),
                    "Rerun verify, assess, and airlock, then run canon strategy promote or register",
                );
                return;
            };
            for (proof, content_hash, proof_path, decision) in [
                (
                    "verify",
                    &proofs.verify.content_hash,
                    &proofs.verify.path,
                    &proofs.verify.decision,
                ),
                (
                    "assess",
                    &proofs.assess.content_hash,
                    &proofs.assess.path,
                    &proofs.assess.decision,
                ),
                (
                    "airlock",
                    &proofs.airlock.content_hash,
                    &proofs.airlock.path,
                    &proofs.airlock.decision,
                ),
            ] {
                if content_hash.trim().is_empty()
                    || proof_path.trim().is_empty()
                    || decision.trim().is_empty()
                {
                    context.finding(
                        RegistryLintSeverity::Error,
                        "strategy_proofs",
                        "strategy_proof_incomplete",
                        format!(
                            "Strategy entry {entry_order} has incomplete {proof} proof reference"
                        ),
                        FindingLocation::path(path),
                        json!({
                            "entry_order": entry_order,
                            "proof": proof,
                            "has_path": !proof_path.trim().is_empty(),
                            "has_content_hash": !content_hash.trim().is_empty(),
                            "has_decision": !decision.trim().is_empty(),
                        }),
                        "Rerun verify, assess, and airlock, then run canon strategy promote or register",
                    );
                }
            }
        }
        StrategyAttestationGrade::OperatorAttested => {
            let Some(attestation) = entry.operator_attestation.as_ref() else {
                context.finding(
                    RegistryLintSeverity::Error,
                    "strategy_attestation",
                    "strategy_operator_attestation_missing",
                    format!(
                        "Strategy entry {entry_order} is operator-attested but has no attestation"
                    ),
                    FindingLocation::path(path),
                    json!({ "entry_order": entry_order }),
                    "Update the entry with canon strategy update or register it again",
                );
                return;
            };
            for (field, value) in [
                ("operator", attestation.operator.as_str()),
                ("attested_at", attestation.attested_at.as_str()),
                ("reason", attestation.reason.as_str()),
                (
                    "script_content_hash",
                    attestation.script_content_hash.as_str(),
                ),
                ("attestation_hash", attestation.attestation_hash.as_str()),
            ] {
                if value.trim().is_empty() {
                    context.finding(
                        RegistryLintSeverity::Error,
                        "strategy_attestation",
                        "strategy_operator_attestation_incomplete",
                        format!(
                            "Strategy entry {entry_order} has incomplete operator attestation field '{field}'"
                        ),
                        FindingLocation::path(path),
                        json!({ "entry_order": entry_order, "field": field }),
                        "Update the entry with canon strategy update or register it again",
                    );
                }
            }
        }
    }

    if entry.status == StrategyEntryStatus::Deprecated && entry.deprecation.is_none() {
        context.finding(
            RegistryLintSeverity::Error,
            "strategy_lifecycle",
            "strategy_deprecation_missing",
            format!("Strategy entry {entry_order} is deprecated but has no deprecation metadata"),
            FindingLocation::path(path),
            json!({ "entry_order": entry_order }),
            "Deprecate the entry again with canon strategy deprecate",
        );
    }
}

fn check_duplicate_strategy_keys(
    context: &mut LintContext,
    records: &[(StrategyRegistryEntry, String, usize)],
) {
    let mut first_by_key = BTreeMap::<StrategyEntryKey, (&str, usize)>::new();
    for (entry, source_file, entry_order) in records {
        if entry.status != StrategyEntryStatus::Active {
            continue;
        }
        let key = entry.key.clone();
        if let Some((first_file, first_order)) = first_by_key.get(&key) {
            context.finding(
                RegistryLintSeverity::Warning,
                "strategy_duplicates",
                "duplicate_strategy_key",
                "Duplicate active strategy key is shadowed by precedence",
                FindingLocation::default(),
                json!({
                    "key": key,
                    "first": { "source_file": first_file, "entry_order": first_order },
                    "shadowed": { "source_file": source_file, "entry_order": entry_order },
                }),
                "Run canon strategy deprecate on shadowed active entries, then rerun canon registry lint",
            );
        } else {
            first_by_key.insert(key, (source_file, *entry_order));
        }
    }
}

fn load_alias_records(context: &mut LintContext, files: &[PathBuf]) -> Vec<MappingRecord> {
    load_mapping_records(context, files, "org alias")
}

fn check_alias_conflicts(context: &mut LintContext, aliases: &[MappingRecord]) {
    check_duplicate_inputs(context, aliases);
}

fn discover_anchor_files(context: &mut LintContext) -> Vec<PathBuf> {
    let anchors_dir = context.registry_dir.join("_anchors");
    if !anchors_dir.exists() {
        return Vec::new();
    }
    if !anchors_dir.is_dir() {
        context.finding(
            RegistryLintSeverity::Error,
            "org_anchors",
            "anchors_path_not_directory",
            "_anchors exists but is not a directory",
            FindingLocation::path(&anchors_dir),
            Value::Null,
            "Replace _anchors with a directory, then rerun canon registry lint",
        );
        return Vec::new();
    }
    discover_jsonl_files_in_dir(context, &anchors_dir, "org_anchors")
}

fn discover_jsonl_files_in_dir(
    context: &mut LintContext,
    dir: &Path,
    category: &str,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            context.finding(
                RegistryLintSeverity::Error,
                category,
                "directory_unreadable",
                format!("Failed to read directory '{}': {error}", dir.display()),
                FindingLocation::path(dir),
                json!({ "error": error.to_string() }),
                "Fix sidecar directory permissions, then rerun canon registry lint",
            );
            return files;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension() == Some("jsonl".as_ref()) {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn load_anchor_records(context: &mut LintContext, files: &[PathBuf]) -> Vec<TrustedAnchorRecord> {
    let mut records = Vec::new();
    for path in files {
        for (line, record) in read_jsonl::<TrustedAnchorRecord>(context, path, "trusted anchor") {
            for (field, value) in [
                ("canonical_id", record.canonical_id.as_str()),
                ("namespace", record.namespace.as_str()),
                ("value", record.value.as_str()),
            ] {
                if value.trim().is_empty() {
                    context.finding(
                        RegistryLintSeverity::Error,
                        "org_anchors",
                        "anchor_required_field_empty",
                        format!("Trusted anchor line {line} has empty required field '{field}'"),
                        FindingLocation::line(path, line),
                        json!({ "field": field }),
                        "Populate trusted-anchor fields, then rerun canon registry lint",
                    );
                }
            }
            records.push(record);
        }
    }
    records
}

fn check_anchor_conflicts(context: &mut LintContext, anchors: &[TrustedAnchorRecord]) {
    let mut first_by_anchor = BTreeMap::<(String, String), &TrustedAnchorRecord>::new();
    for anchor in anchors {
        let key = (anchor.namespace.clone(), anchor.value.clone());
        if let Some(first) = first_by_anchor.get(&key) {
            if first.canonical_id != anchor.canonical_id {
                context.finding(
                    RegistryLintSeverity::Error,
                    "org_anchors",
                    "anchor_conflict",
                    "Trusted anchor value maps to multiple canonical IDs",
                    FindingLocation::default(),
                    json!({
                        "namespace": key.0,
                        "value": key.1,
                        "first_canonical_id": first.canonical_id,
                        "conflicting_canonical_id": anchor.canonical_id,
                    }),
                    "Resolve the trusted-anchor conflict before running canon entity",
                );
            }
        } else {
            first_by_anchor.insert(key, anchor);
        }
    }
}

fn load_pending_records(context: &mut LintContext) -> Vec<PendingClusterRecord> {
    let Some(path) = file_if_exists(context.registry_dir.join("_escrow/pending.jsonl")) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for (line, raw) in read_jsonl::<RawPendingClusterRecord>(context, &path, "pending escrow") {
        for (field, value) in [
            ("escrow_id", raw.escrow_id.as_str()),
            ("profile", raw.profile.as_str()),
            ("state", raw.state.as_str()),
        ] {
            if value.trim().is_empty() {
                context.finding(
                    RegistryLintSeverity::Error,
                    "org_escrow",
                    "pending_required_field_empty",
                    format!("Pending escrow line {line} has empty required field '{field}'"),
                    FindingLocation::line(&path, line),
                    json!({ "field": field }),
                    "Populate pending escrow fields, then rerun canon registry lint",
                );
            }
        }
        for (anchor_index, anchor) in raw.anchors.iter().enumerate() {
            if anchor.namespace.trim().is_empty() || anchor.value.trim().is_empty() {
                context.finding(
                    RegistryLintSeverity::Error,
                    "org_escrow",
                    "pending_anchor_incomplete",
                    format!("Pending escrow line {line} has incomplete anchor"),
                    FindingLocation::line(&path, line),
                    json!({ "anchor_index": anchor_index }),
                    "Populate pending escrow anchor namespace/value, then rerun canon registry lint",
                );
            }
        }
        let witness_pairs = raw
            .witness_pairs
            .into_iter()
            .map(RawRowPair::into_row_pair)
            .collect::<Vec<_>>();
        records.push(PendingClusterRecord {
            escrow_id: raw.escrow_id,
            profile: raw.profile,
            doc_ids: raw.doc_ids,
            surfaces: raw.surfaces,
            anchors: raw.anchors,
            witness_pairs,
            state: raw.state,
        });
    }
    records
}

fn check_pending_conflicts(context: &mut LintContext, pending: &[PendingClusterRecord]) {
    let mut seen = BTreeSet::new();
    for cluster in pending {
        if !seen.insert(cluster.escrow_id.clone()) {
            context.finding(
                RegistryLintSeverity::Warning,
                "org_escrow",
                "duplicate_pending_escrow_id",
                format!("Duplicate pending escrow id '{}'", cluster.escrow_id),
                FindingLocation::default(),
                json!({ "escrow_id": cluster.escrow_id }),
                "Deduplicate pending escrow records, then rerun canon registry lint",
            );
        }
    }
}

fn load_cannot_link_records(context: &mut LintContext) -> Vec<CannotLinkFact> {
    let Some(path) = file_if_exists(context.registry_dir.join("_escrow/cannot_link.jsonl")) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for (line, record) in read_jsonl::<CannotLinkFact>(context, &path, "cannot-link") {
        for (field, value) in [
            ("left_key", record.left_key.as_str()),
            ("right_key", record.right_key.as_str()),
            ("reason", record.reason.as_str()),
        ] {
            if value.trim().is_empty() {
                context.finding(
                    RegistryLintSeverity::Error,
                    "org_escrow",
                    "cannot_link_required_field_empty",
                    format!("Cannot-link line {line} has empty required field '{field}'"),
                    FindingLocation::line(&path, line),
                    json!({ "field": field }),
                    "Populate cannot-link fields, then rerun canon registry lint",
                );
            }
        }
        records.push(record);
    }
    records
}

fn check_cannot_link_conflicts(context: &mut LintContext, facts: &[CannotLinkFact]) {
    let mut seen = BTreeMap::<(String, String), &str>::new();
    for fact in facts {
        let key = ordered_pair(&fact.left_key, &fact.right_key);
        if key.0 == key.1 {
            context.finding(
                RegistryLintSeverity::Error,
                "org_escrow",
                "cannot_link_self_conflict",
                "Cannot-link fact points to the same key on both sides",
                FindingLocation::default(),
                json!({ "key": key.0, "reason": fact.reason }),
                "Remove impossible self cannot-link records, then rerun canon registry lint",
            );
        }
        if let Some(first_reason) = seen.get(&key) {
            if *first_reason != fact.reason {
                context.finding(
                    RegistryLintSeverity::Warning,
                    "org_escrow",
                    "cannot_link_duplicate_pair",
                    "Cannot-link pair appears with multiple reasons",
                    FindingLocation::default(),
                    json!({
                        "left_key": key.0,
                        "right_key": key.1,
                        "first_reason": first_reason,
                        "duplicate_reason": fact.reason,
                    }),
                    "Deduplicate cannot-link facts, then rerun canon registry lint",
                );
            }
        } else {
            seen.insert(key, &fact.reason);
        }
    }
}

fn set_org_snapshot_hashes(
    context: &mut LintContext,
    alias_files: &[PathBuf],
    anchor_files: &[PathBuf],
) {
    let mut lookup_paths = Vec::with_capacity(1 + alias_files.len() + anchor_files.len());
    lookup_paths.push(context.registry_dir.join("registry.json"));
    lookup_paths.extend(alias_files.iter().cloned());
    lookup_paths.extend(anchor_files.iter().cloned());
    context.registry.lookup_snapshot_hash =
        manifest_hash(context, &lookup_paths, "lookup snapshot");

    let escrow_paths = [
        context.registry_dir.join("_escrow/cannot_link.jsonl"),
        context.registry_dir.join("_escrow/pending.jsonl"),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .collect::<Vec<_>>();
    context.registry.escrow_snapshot_hash =
        manifest_hash(context, &escrow_paths, "escrow snapshot");
}

fn read_jsonl<T>(context: &mut LintContext, path: &Path, label: &str) -> Vec<(usize, T)>
where
    T: DeserializeOwned,
{
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            context.finding(
                RegistryLintSeverity::Error,
                "sidecar_parse",
                "sidecar_unreadable",
                format!(
                    "Failed to read {label} sidecar '{}': {error}",
                    path.display()
                ),
                FindingLocation::path(path),
                json!({ "error": error.to_string() }),
                "Fix sidecar permissions, then rerun canon registry lint",
            );
            return Vec::new();
        }
    };
    let mut records = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(record) => records.push((line_number, record)),
            Err(error) => context.finding(
                RegistryLintSeverity::Error,
                "sidecar_parse",
                "sidecar_record_malformed",
                format!("Failed to parse {label} sidecar line {line_number}: {error}"),
                FindingLocation::line(path, line_number),
                json!({ "error": error.to_string() }),
                "Fix sidecar JSONL records, then rerun canon registry lint",
            ),
        }
    }
    records
}

#[derive(Debug, Deserialize)]
struct RawPendingClusterRecord {
    escrow_id: String,
    profile: String,
    #[serde(default)]
    doc_ids: Vec<String>,
    #[serde(default)]
    surfaces: Vec<String>,
    #[serde(default)]
    anchors: Vec<AnchorValue>,
    #[serde(default)]
    witness_pairs: Vec<RawRowPair>,
    state: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawRowPair {
    Object {
        left_row_id: String,
        right_row_id: String,
    },
    Tuple([String; 2]),
}

impl RawRowPair {
    fn into_row_pair(self) -> RowPair {
        match self {
            Self::Object {
                left_row_id,
                right_row_id,
            } => RowPair {
                left_row_id,
                right_row_id,
            },
            Self::Tuple([left_row_id, right_row_id]) => RowPair {
                left_row_id,
                right_row_id,
            },
        }
    }
}

fn manifest_hash(context: &mut LintContext, paths: &[PathBuf], label: &str) -> Option<String> {
    let mut manifest = Vec::new();
    for path in paths {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                context.finding(
                    RegistryLintSeverity::Error,
                    "snapshot_hash",
                    "snapshot_input_unreadable",
                    format!("Failed to read {label} input '{}': {error}", path.display()),
                    FindingLocation::path(path),
                    json!({ "error": error.to_string() }),
                    "Fix snapshot input files, then rerun canon registry lint",
                );
                return None;
            }
        };
        let relative_path = relative_path(&context.registry_dir, path);
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        manifest.extend_from_slice(relative_path.as_bytes());
        manifest.push(b'\t');
        manifest.extend_from_slice(bytes.len().to_string().as_bytes());
        manifest.push(b'\t');
        manifest.extend_from_slice(content_hash.as_bytes());
        manifest.push(b'\n');
    }
    Some(format!("blake3:{}", blake3::hash(&manifest).to_hex()))
}

fn read_index_metadata_value(conn: &Connection, key: &str) -> rusqlite::Result<String> {
    let mut statement = conn.prepare("SELECT value FROM metadata WHERE key = ?1")?;
    let mut rows = statement.query([key])?;
    match rows.next()? {
        Some(row) => row.get::<_, String>(0),
        None => Err(SqliteError::QueryReturnedNoRows),
    }
}

fn max_json_mtime<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> Option<u64> {
    paths
        .filter_map(|path| fs::metadata(path).ok())
        .filter_map(|metadata| metadata.modified().ok())
        .filter_map(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .max()
}

fn summarize_findings(
    findings: &[RegistryLintFinding],
    profile: RegistryLintProfile,
) -> RegistryLintSummary {
    let mut severity_counts = BTreeMap::<&str, usize>::new();
    let mut by_category = BTreeMap::new();
    for finding in findings {
        *severity_counts
            .entry(finding.severity.as_count_key())
            .or_default() += 1;
        *by_category.entry(finding.category.clone()).or_default() += 1;
    }
    RegistryLintSummary {
        total_findings: findings.len(),
        errors: *severity_counts.get("error").unwrap_or(&0),
        warnings: *severity_counts.get("warning").unwrap_or(&0),
        info: *severity_counts.get("info").unwrap_or(&0),
        checked_profiles: vec![profile.as_str().to_string()],
        by_category,
    }
}

fn next_command_for_summary(summary: &RegistryLintSummary) -> &'static str {
    if summary.errors > 0 {
        "Fix error findings, then rerun canon registry lint"
    } else if summary.warnings > 0 {
        "Review warning findings before production use"
    } else {
        "No registry lint action required"
    }
}

fn standalone_finding(
    severity: RegistryLintSeverity,
    category: &str,
    code: &str,
    message: impl Into<String>,
    location: FindingLocation,
    detail: Value,
    next_command: impl Into<String>,
) -> RegistryLintFinding {
    RegistryLintFinding {
        severity,
        category: category.to_string(),
        code: code.to_string(),
        message: message.into(),
        path: location.path.map(|path| path.display().to_string()),
        line: location.line,
        detail,
        next_command: next_command.into(),
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn mapping_record_detail(record: &MappingRecord) -> Value {
    json!({
        "source_file": record.source_file,
        "entry_order": record.entry_order,
        "canonical_id": record.canonical_id,
        "canonical_type": record.canonical_type,
        "rule_id": record.rule_id,
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn file_if_exists(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

fn ordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn hash_json<T: Serialize>(value: &T) -> String {
    let bytes =
        serde_json::to_vec(value).expect("serializing registry lint hash input is infallible");
    format!("blake3:{}", blake3::hash(&bytes).to_hex())
}

#[allow(dead_code)]
fn _assert_schema_shape_hash_input(_: &StrategySchemaShape) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy_registry::{
        FrozenScript, StrategyColumn, StrategyProofReference, StrategyProofs,
    };
    use std::{error::Error, fs};
    use tempfile::TempDir;

    fn write_registry_metadata(
        dir: &Path,
        id: &str,
        version: &str,
        entry_count: usize,
    ) -> Result<(), Box<dyn Error>> {
        fs::write(
            dir.join("registry.json"),
            serde_json::to_string_pretty(&json!({
                "id": id,
                "version": version,
                "description": "Test registry",
                "updated": "2026-05-06",
                "entry_count": entry_count,
            }))?,
        )?;
        Ok(())
    }

    fn write_json(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
        fs::write(path, serde_json::to_string_pretty(value)?)?;
        Ok(())
    }

    fn finding_codes(output: &RegistryLintOutput) -> Vec<String> {
        output
            .findings
            .iter()
            .map(|finding| finding.code.clone())
            .collect()
    }

    #[test]
    fn standard_lint_reports_clean_registry_without_errors_or_warnings()
    -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new()?;
        write_registry_metadata(temp.path(), "standard", "1.0.0", 2)?;
        write_json(
            &temp.path().join("mappings.json"),
            &json!([
                {"input":"A","canonical_id":"C1","canonical_type":"entity","rule_id":"r1"},
                {"input":"B","canonical_id":"C2","canonical_type":"entity","rule_id":"r1"}
            ]),
        )?;

        let output = lint(temp.path(), RegistryLintProfile::Standard).expect("lint standard");

        assert_eq!(output.version, "canon_registry_lint.v0");
        assert_eq!(output.profile, "standard");
        assert_eq!(output.summary.errors, 0);
        assert_eq!(output.summary.warnings, 0);
        assert!(finding_codes(&output).contains(&"index_missing".to_string()));
        Ok(())
    }

    #[test]
    fn standard_lint_reports_dirty_registry_findings() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new()?;
        write_registry_metadata(temp.path(), "standard", "1.0.0", 4)?;
        write_json(
            &temp.path().join("a.json"),
            &json!([
                {"input":"A","canonical_id":"C1","canonical_type":"entity","rule_id":"r1"},
                {"input":"B","canonical_id":"","canonical_type":"entity","rule_id":"r1"}
            ]),
        )?;
        write_json(
            &temp.path().join("b.json"),
            &json!([
                {"input":"A","canonical_id":"C9","canonical_type":"entity","rule_id":"r2"}
            ]),
        )?;

        let output = lint(temp.path(), RegistryLintProfile::Standard).expect("lint standard");
        let codes = finding_codes(&output);

        assert!(codes.contains(&"entry_count_stale".to_string()));
        assert!(codes.contains(&"mapping_required_field_empty".to_string()));
        assert!(codes.contains(&"shadowed_input".to_string()));
        assert!(output.summary.errors > 0);
        assert!(output.summary.warnings > 0);
        Ok(())
    }

    #[test]
    fn strategy_lint_reports_clean_and_dirty_entries() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new()?;
        write_registry_metadata(temp.path(), "strategy", "1.0.0", 2)?;
        let strategy_dir = temp.path().join("_strategy");
        fs::create_dir(&strategy_dir)?;
        let schema = StrategySchemaShape {
            columns: vec![StrategyColumn {
                name: "name".to_string(),
                kind: "string".to_string(),
                cardinality: Some(2),
            }],
        };
        let good_fingerprint = hash_json(&schema);
        let good = strategy_entry(
            &schema,
            "skill-a",
            good_fingerprint.clone(),
            "script-a",
            "hash-a",
        );
        let dirty_schema = StrategySchemaShape {
            columns: vec![StrategyColumn {
                name: "other_name".to_string(),
                kind: "string".to_string(),
                cardinality: Some(2),
            }],
        };
        let mut dirty = strategy_entry(&dirty_schema, "skill-a", good_fingerprint, "", "");
        dirty.proofs.as_mut().unwrap().verify.content_hash.clear();
        fs::write(
            strategy_dir.join("entries.json"),
            serde_json::to_string_pretty(&vec![good, dirty])?,
        )?;

        let output = lint(temp.path(), RegistryLintProfile::Strategy).expect("lint strategy");
        let codes = finding_codes(&output);

        assert_eq!(output.profile, "strategy");
        assert!(codes.contains(&"strategy_required_field_empty".to_string()));
        assert!(codes.contains(&"strategy_proof_incomplete".to_string()));
        assert!(codes.contains(&"schema_fingerprint_mismatch".to_string()));
        assert!(codes.contains(&"duplicate_strategy_key".to_string()));
        Ok(())
    }

    #[test]
    fn org_lint_reports_clean_registry_snapshot_hashes() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new()?;
        write_registry_metadata(temp.path(), "org", "1.0.0", 1)?;
        write_json(
            &temp.path().join("aliases.json"),
            &json!([
                {"input":"Acme","canonical_id":"ORG-1","canonical_type":"org","rule_id":"alias"}
            ]),
        )?;
        fs::create_dir(temp.path().join("_anchors"))?;
        fs::write(
            temp.path().join("_anchors/lei.jsonl"),
            "{\"canonical_id\":\"ORG-1\",\"namespace\":\"lei\",\"value\":\"LEI1\"}\n",
        )?;

        let output = lint(temp.path(), RegistryLintProfile::Org).expect("lint org");

        assert_eq!(output.summary.errors, 0);
        assert_eq!(output.summary.warnings, 0);
        assert!(output.registry.lookup_snapshot_hash.is_some());
        assert!(output.registry.escrow_snapshot_hash.is_some());
        Ok(())
    }

    #[test]
    fn org_lint_reports_dirty_sidecar_conflicts() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new()?;
        write_registry_metadata(temp.path(), "org", "1.0.0", 2)?;
        write_json(
            &temp.path().join("aliases.json"),
            &json!([
                {"input":"Acme","canonical_id":"ORG-1","canonical_type":"org","rule_id":"alias"},
                {"input":"Acme","canonical_id":"ORG-2","canonical_type":"org","rule_id":"alias"}
            ]),
        )?;
        fs::create_dir(temp.path().join("_anchors"))?;
        fs::write(
            temp.path().join("_anchors/lei.jsonl"),
            "{\"canonical_id\":\"ORG-1\",\"namespace\":\"lei\",\"value\":\"LEI1\"}\n{\"canonical_id\":\"ORG-2\",\"namespace\":\"lei\",\"value\":\"LEI1\"}\n",
        )?;
        fs::create_dir(temp.path().join("_escrow"))?;
        fs::write(
            temp.path().join("_escrow/pending.jsonl"),
            "{\"escrow_id\":\"E1\",\"profile\":\"bdc\",\"state\":\"pending\"}\n{\"escrow_id\":\"E1\",\"profile\":\"bdc\",\"state\":\"pending\"}\n",
        )?;
        fs::write(
            temp.path().join("_escrow/cannot_link.jsonl"),
            "{\"left_key\":\"ORG-1\",\"right_key\":\"ORG-1\",\"reason\":\"bad\"}\n",
        )?;

        let output = lint(temp.path(), RegistryLintProfile::Org).expect("lint org");
        let codes = finding_codes(&output);

        assert!(codes.contains(&"shadowed_input".to_string()));
        assert!(codes.contains(&"anchor_conflict".to_string()));
        assert!(codes.contains(&"duplicate_pending_escrow_id".to_string()));
        assert!(codes.contains(&"cannot_link_self_conflict".to_string()));
        assert!(output.summary.errors > 0);
        assert!(output.summary.warnings > 0);
        Ok(())
    }

    #[test]
    fn auto_profile_prefers_strategy_then_org_then_standard() -> Result<(), Box<dyn Error>> {
        let standard = TempDir::new()?;
        write_registry_metadata(standard.path(), "standard", "1.0.0", 0)?;
        assert_eq!(
            lint(standard.path(), RegistryLintProfile::Auto)
                .expect("lint standard auto")
                .profile,
            "standard"
        );

        let org = TempDir::new()?;
        write_registry_metadata(org.path(), "org", "1.0.0", 0)?;
        fs::create_dir(org.path().join("_anchors"))?;
        assert_eq!(
            lint(org.path(), RegistryLintProfile::Auto)
                .expect("lint org auto")
                .profile,
            "org"
        );

        let strategy = TempDir::new()?;
        write_registry_metadata(strategy.path(), "strategy", "1.0.0", 0)?;
        fs::create_dir(strategy.path().join("_strategy"))?;
        assert_eq!(
            lint(strategy.path(), RegistryLintProfile::Auto)
                .expect("lint strategy auto")
                .profile,
            "strategy"
        );
        Ok(())
    }

    fn strategy_entry(
        schema: &StrategySchemaShape,
        skill_hash: &str,
        schema_fingerprint: String,
        script_id: &str,
        script_hash: &str,
    ) -> StrategyRegistryEntry {
        StrategyRegistryEntry {
            entry_schema_version: "canon_strategy_entry.v1".to_string(),
            key: StrategyEntryKey::Schema {
                schema_fingerprint,
                skill_hash: skill_hash.to_string(),
            },
            schema: Some(schema.clone()),
            grade: StrategyAttestationGrade::ProofAttested,
            status: StrategyEntryStatus::Active,
            skill_hash: skill_hash.to_string(),
            script: FrozenScript {
                id: script_id.to_string(),
                path: "script.py".to_string(),
                language: "python".to_string(),
                content_hash: script_hash.to_string(),
            },
            proofs: Some(StrategyProofs {
                verify: proof("verify"),
                assess: proof("assess"),
                airlock: proof("airlock"),
            }),
            operator_attestation: None,
            deprecation: None,
            rule_id: "RULE".to_string(),
        }
    }

    fn proof(name: &str) -> StrategyProofReference {
        StrategyProofReference {
            path: format!("{name}.json"),
            content_hash: format!("blake3:{name}"),
            decision: "PASS".to_string(),
        }
    }
}
