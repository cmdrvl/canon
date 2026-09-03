use super::{
    DefaultIdScheme, MappingFile, PlannedFileMutation, PlannedMutationState,
    acquire_registry_mutation_guard, hash_bytes, load_registry_definition,
    validate_planned_mutations,
};
use crate::{
    Refusal, RefusalCode, RegistryMeta,
    identity_scope::IdentityScope,
    registry_lint::{RegistryLintOutput, RegistryLintProfile, RegistryLintSeverity},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TEMP_FILE_STALE_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryVersionBump {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Clone)]
pub struct RegistryAddEntryRequest {
    pub registry: PathBuf,
    pub alias_file: String,
    pub canonical_id: String,
    pub input: String,
    pub rule_id: String,
    pub canonical_type: Option<String>,
    pub bump: Option<RegistryVersionBump>,
    pub next_version: Option<String>,
    pub no_lint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryAddEntryRegistry {
    pub id: String,
    pub source: String,
    pub version_before: String,
    pub version_after: String,
    pub entry_count_before: usize,
    pub entry_count_after: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryAddEntryAliasEntry {
    pub alias_file: String,
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RegistryAliasWriteEntry {
    pub alias_file: String,
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
    pub scope: Option<IdentityScope>,
}

impl RegistryAliasWriteEntry {
    pub(super) fn output_entry(&self) -> RegistryAddEntryAliasEntry {
        RegistryAddEntryAliasEntry {
            alias_file: self.alias_file.clone(),
            input: self.input.clone(),
            canonical_id: self.canonical_id.clone(),
            canonical_type: self.canonical_type.clone(),
            rule_id: self.rule_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryAddEntryLintSummary {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub total_findings: usize,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryAddEntryOutput {
    pub version: String,
    pub registry: RegistryAddEntryRegistry,
    pub alias_entry: RegistryAddEntryAliasEntry,
    pub touched_files: Vec<String>,
    pub lint: RegistryAddEntryLintSummary,
    pub warnings: Vec<String>,
}

impl RegistryAddEntryOutput {
    pub fn render_plain(&self) -> String {
        format!(
            "added {} -> {} in {} ({})",
            self.alias_entry.input,
            self.alias_entry.canonical_id,
            self.alias_entry.alias_file,
            self.registry.version_after
        )
    }
}

#[derive(Debug, Clone)]
pub struct RegistryAddEntryPlan {
    pub registry_path: PathBuf,
    pub alias_path: PathBuf,
    expected_registry_hash: String,
    expected_alias_hash: String,
    pub registry_bytes: Vec<u8>,
    pub alias_bytes: Vec<u8>,
    pub lint_enabled: bool,
    pub output: RegistryAddEntryOutput,
}

pub fn add_entry(request: RegistryAddEntryRequest) -> Result<RegistryAddEntryOutput, Refusal> {
    add_entry_with_scope(request, None)
}

pub fn add_entry_with_scope(
    request: RegistryAddEntryRequest,
    scope: Option<IdentityScope>,
) -> Result<RegistryAddEntryOutput, Refusal> {
    let plan = plan_add_entry_with_scope(request, scope)?;
    commit_add_entry_plan(plan)
}

pub fn plan_add_entry(request: RegistryAddEntryRequest) -> Result<RegistryAddEntryPlan, Refusal> {
    plan_add_entry_with_scope(request, None)
}

pub fn plan_add_entry_with_scope(
    request: RegistryAddEntryRequest,
    scope: Option<IdentityScope>,
) -> Result<RegistryAddEntryPlan, Refusal> {
    let _guard = acquire_registry_mutation_guard(&request.registry)
        .map_err(|error| io_refusal(&request.registry, error))?;
    let scope = finalize_requested_scope(
        &request.registry,
        scope,
        "canon registry add-entry --scope DIMENSION=VALUE ...",
    )?;
    let registry_path = request.registry.join("registry.json");
    let (registry_json, registry_meta, mapping_files) = load_registry_definition(&request.registry)
        .map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;

    let alias_path = validate_alias_file(&request.registry, &request.alias_file)?;
    let input = validate_trimmed_non_empty(
        &request.registry,
        "--input",
        &request.input,
        true,
        "canon registry add-entry --input <TRIMMED_INPUT> ...",
    )?;
    let canonical_id = validate_trimmed_non_empty(
        &request.registry,
        "--canonical-id",
        &request.canonical_id,
        false,
        "canon registry add-entry --canonical-id <ID> ...",
    )?;
    let rule_id = validate_trimmed_non_empty(
        &request.registry,
        "--rule-id",
        &request.rule_id,
        false,
        "canon registry add-entry --rule-id <RULE> ...",
    )?;

    validate_default_id_scheme(
        &request.registry,
        &canonical_id,
        registry_json.default_id_scheme.as_ref(),
    )?;
    ensure_input_is_new_in_scope(&request.registry, &input, scope.as_ref(), &mapping_files)?;
    let canonical_type = resolve_canonical_type(
        &request.registry,
        &canonical_id,
        request.canonical_type.as_deref(),
        &mapping_files,
    )?;
    let version_after = resolve_next_version(
        &request.registry,
        &registry_json.version,
        request.bump,
        request.next_version.as_deref(),
    )?;

    let entry_count_after = registry_json.entry_count.checked_add(1).ok_or_else(|| {
        bad_registry_refusal(
            &request.registry,
            "Registry entry_count is too large to increment",
            json!({
                "entry_count": registry_json.entry_count,
            }),
            "Repair registry.json entry_count, then rerun",
        )
    })?;
    let registry_source_bytes =
        fs::read(&registry_path).map_err(|error| io_refusal(&registry_path, error))?;
    let alias_source_bytes =
        fs::read(&alias_path).map_err(|error| io_refusal(&alias_path, error))?;

    let registry_bytes = build_registry_bytes_from_source(
        &request.registry,
        &registry_source_bytes,
        &version_after,
        entry_count_after,
    )?;
    let alias_bytes = build_alias_bytes_from_source(
        &request.registry,
        &alias_path,
        &alias_source_bytes,
        &input,
        &canonical_id,
        &canonical_type,
        &rule_id,
        scope.as_ref(),
    )?;

    let alias_file = request.alias_file;
    let output = RegistryAddEntryOutput {
        version: "canon_registry_add_entry.v0".to_string(),
        registry: registry_output(
            registry_meta,
            registry_json.version,
            version_after,
            registry_json.entry_count,
            entry_count_after,
        ),
        alias_entry: RegistryAddEntryAliasEntry {
            alias_file: alias_file.clone(),
            input,
            canonical_id,
            canonical_type,
            rule_id,
        },
        touched_files: vec![alias_file, "registry.json".to_string()],
        lint: RegistryAddEntryLintSummary {
            enabled: !request.no_lint,
            profile: None,
            total_findings: 0,
            errors: 0,
            warnings: 0,
            info: 0,
        },
        warnings: Vec::new(),
    };

    Ok(RegistryAddEntryPlan {
        registry_path,
        alias_path,
        expected_registry_hash: hash_bytes(&registry_source_bytes),
        expected_alias_hash: hash_bytes(&alias_source_bytes),
        registry_bytes,
        alias_bytes,
        lint_enabled: !request.no_lint,
        output,
    })
}

pub(super) fn finalize_requested_scope(
    registry: &Path,
    scope: Option<IdentityScope>,
    next_command: &str,
) -> Result<Option<IdentityScope>, Refusal> {
    super::finalize_mapping_scope_metadata(None, scope).map_err(|error| {
        parse_refusal(
            registry,
            "Invalid --scope for registry mutation",
            json!({ "scope_error": error }),
            next_command,
        )
    })
}

fn registry_output(
    registry_meta: RegistryMeta,
    version_before: String,
    version_after: String,
    entry_count_before: usize,
    entry_count_after: usize,
) -> RegistryAddEntryRegistry {
    RegistryAddEntryRegistry {
        id: registry_meta.id,
        source: registry_meta.source,
        version_before,
        version_after,
        entry_count_before,
        entry_count_after,
    }
}

pub(super) fn validate_alias_file(registry: &Path, alias_file: &str) -> Result<PathBuf, Refusal> {
    let alias_path = Path::new(alias_file);
    let has_single_component = alias_path
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
        && alias_path.file_name() == Some(OsStr::new(alias_file));
    let valid_name = alias_file.ends_with(".json")
        && alias_file != "registry.json"
        && alias_file != "_build.json"
        && has_single_component;
    if !valid_name {
        return Err(parse_refusal(
            registry,
            "Invalid --alias-file for registry add-entry",
            json!({
                "alias_file": alias_file,
                "expected": "existing root-level mapping file ending in .json, excluding registry.json and _build.json",
            }),
            "canon registry add-entry --alias-file aliases.json ...",
        ));
    }

    let full_path = registry.join(alias_file);
    if !full_path.is_file() {
        return Err(parse_refusal(
            registry,
            "Registry add-entry alias file must already exist",
            json!({
                "alias_file": alias_file,
                "path": full_path.display().to_string(),
            }),
            "Create an empty root alias file such as [] before rerunning",
        ));
    }

    Ok(full_path)
}

pub(super) fn validate_trimmed_non_empty(
    registry: &Path,
    flag: &str,
    value: &str,
    require_already_trimmed: bool,
    next_command: &str,
) -> Result<String, Refusal> {
    let trimmed = ascii_trim(value);
    if trimmed.is_empty() {
        return Err(parse_refusal(
            registry,
            format!("{flag} must not be empty after ASCII trim"),
            json!({
                "flag": flag,
            }),
            next_command,
        ));
    }
    if require_already_trimmed && trimmed != value {
        return Err(parse_refusal(
            registry,
            format!("{flag} must already be ASCII-trimmed"),
            json!({
                "flag": flag,
                "input": value,
                "trimmed": trimmed,
            }),
            next_command,
        ));
    }
    Ok(trimmed.to_string())
}

pub(super) fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

pub(super) fn validate_default_id_scheme(
    registry: &Path,
    canonical_id: &str,
    scheme: Option<&DefaultIdScheme>,
) -> Result<(), Refusal> {
    let Some(scheme) = scheme else {
        return Ok(());
    };
    if scheme.zero_pad == 0 {
        return Err(bad_registry_refusal(
            registry,
            "registry.json default_id_scheme.zero_pad must be greater than zero",
            json!({
                "prefix": scheme.prefix,
                "zero_pad": scheme.zero_pad,
            }),
            "Repair default_id_scheme.zero_pad, then rerun",
        ));
    }
    let namespace = format!("{}-", scheme.prefix);
    let Some(suffix) = canonical_id.strip_prefix(&namespace) else {
        return Err(parse_refusal(
            registry,
            "Canonical ID does not match registry.json default_id_scheme prefix",
            json!({
                "canonical_id": canonical_id,
                "prefix": scheme.prefix,
            }),
            format!(
                "Use a {}-* canonical ID or update default_id_scheme before rerunning",
                scheme.prefix
            ),
        ));
    };
    if suffix.len() < scheme.zero_pad
        || suffix.is_empty()
        || !suffix.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(parse_refusal(
            registry,
            "Canonical ID does not match registry.json default_id_scheme numeric suffix",
            json!({
                "canonical_id": canonical_id,
                "prefix": scheme.prefix,
                "zero_pad": scheme.zero_pad,
                "expected": format!("{}-<at least {} digits>", scheme.prefix, scheme.zero_pad),
            }),
            format!(
                "Use canon registry next-id {} --registry {} to choose the next in-scheme ID",
                scheme.prefix,
                registry.display()
            ),
        ));
    }
    Ok(())
}

pub(super) fn ensure_input_is_new_in_scope(
    registry: &Path,
    input: &str,
    proposed_scope: Option<&IdentityScope>,
    mapping_files: &[MappingFile],
) -> Result<(), Refusal> {
    for mapping_file in mapping_files {
        for entry in &mapping_file.entries {
            if entry.input == input && scopes_collide(entry.scope.as_ref(), proposed_scope) {
                return Err(parse_refusal(
                    registry,
                    "Registry already contains this input value",
                    json!({
                        "input": input,
                        "existing": {
                            "canonical_id": entry.canonical_id,
                            "canonical_type": entry.canonical_type,
                            "rule_id": entry.rule_id,
                            "source_file": mapping_file.path.display().to_string(),
                            "scope": entry.scope,
                        }
                    }),
                    "Choose a new input alias or edit the existing mapping intentionally",
                ));
            }
        }
    }
    Ok(())
}

fn scopes_collide(
    existing_scope: Option<&IdentityScope>,
    proposed_scope: Option<&IdentityScope>,
) -> bool {
    match (existing_scope, proposed_scope) {
        (Some(existing), Some(proposed)) => existing == proposed,
        _ => true,
    }
}

pub(super) fn resolve_canonical_type(
    registry: &Path,
    canonical_id: &str,
    provided: Option<&str>,
    mapping_files: &[MappingFile],
) -> Result<String, Refusal> {
    let mut existing_types = BTreeSet::new();
    for mapping_file in mapping_files {
        for entry in &mapping_file.entries {
            if entry.canonical_id == canonical_id {
                existing_types.insert(entry.canonical_type.clone());
            }
        }
    }

    if let Some(provided) = provided {
        let canonical_type = validate_trimmed_non_empty(
            registry,
            "--canonical-type",
            provided,
            false,
            "canon registry add-entry --canonical-type <TYPE> ...",
        )?;
        if !existing_types.is_empty() && !existing_types.contains(&canonical_type) {
            return Err(parse_refusal(
                registry,
                "Canonical ID already uses a different canonical_type",
                json!({
                    "canonical_id": canonical_id,
                    "provided_canonical_type": canonical_type,
                    "existing_canonical_types": existing_types,
                }),
                "Use the existing canonical_type for this canonical ID or choose a new ID",
            ));
        }
        return Ok(canonical_type);
    }

    match existing_types.len() {
        1 => Ok(existing_types.into_iter().next().unwrap()),
        0 => Err(parse_refusal(
            registry,
            "--canonical-type is required for a new canonical ID",
            json!({
                "canonical_id": canonical_id,
            }),
            "canon registry add-entry --canonical-type <TYPE> ...",
        )),
        _ => Err(parse_refusal(
            registry,
            "Cannot infer --canonical-type because existing entries disagree",
            json!({
                "canonical_id": canonical_id,
                "existing_canonical_types": existing_types,
            }),
            "Rerun with an explicit --canonical-type after repairing the registry",
        )),
    }
}

pub(super) fn resolve_next_version(
    registry: &Path,
    current: &str,
    bump: Option<RegistryVersionBump>,
    next_version: Option<&str>,
) -> Result<String, Refusal> {
    if let Some(next_version) = next_version {
        let trimmed = ascii_trim(next_version);
        if trimmed.is_empty() {
            return Err(parse_refusal(
                registry,
                "--next-version must not be empty",
                json!({ "next_version": next_version }),
                "canon registry add-entry --next-version <VERSION> ...",
            ));
        }
        if trimmed == current {
            return Err(parse_refusal(
                registry,
                "--next-version must differ from the current registry version",
                json!({ "current_version": current, "next_version": trimmed }),
                "Choose a new registry version, then rerun",
            ));
        }
        return Ok(trimmed.to_string());
    }

    let bump = bump.unwrap_or(RegistryVersionBump::Patch);
    let mut parts = current.split('.');
    let major = parse_version_part(registry, current, parts.next())?;
    let minor = parse_version_part(registry, current, parts.next())?;
    let patch = parse_version_part(registry, current, parts.next())?;
    if parts.next().is_some() {
        return Err(version_bump_refusal(registry, current));
    }

    let (major, minor, patch) = match bump {
        RegistryVersionBump::Patch => {
            (major, minor, checked_add_version(registry, current, patch)?)
        }
        RegistryVersionBump::Minor => (major, checked_add_version(registry, current, minor)?, 0),
        RegistryVersionBump::Major => (checked_add_version(registry, current, major)?, 0, 0),
    };
    Ok(format!("{major}.{minor}.{patch}"))
}

fn parse_version_part(registry: &Path, current: &str, part: Option<&str>) -> Result<u64, Refusal> {
    let Some(part) = part else {
        return Err(version_bump_refusal(registry, current));
    };
    part.parse::<u64>()
        .map_err(|_| version_bump_refusal(registry, current))
}

fn checked_add_version(registry: &Path, current: &str, value: u64) -> Result<u64, Refusal> {
    value
        .checked_add(1)
        .ok_or_else(|| version_bump_refusal(registry, current))
}

fn version_bump_refusal(registry: &Path, current: &str) -> Refusal {
    parse_refusal(
        registry,
        "Registry version cannot be bumped automatically",
        json!({
            "current_version": current,
            "expected": "MAJOR.MINOR.PATCH numeric version",
        }),
        "Rerun with --next-version <VERSION>",
    )
}

pub(super) fn build_registry_bytes(
    registry: &Path,
    registry_path: &Path,
    version_after: &str,
    entry_count_after: usize,
) -> Result<Vec<u8>, Refusal> {
    let bytes = fs::read(registry_path).map_err(|error| io_refusal(registry_path, error))?;
    build_registry_bytes_from_source(registry, &bytes, version_after, entry_count_after)
}

fn build_registry_bytes_from_source(
    registry: &Path,
    source_bytes: &[u8],
    version_after: &str,
    entry_count_after: usize,
) -> Result<Vec<u8>, Refusal> {
    let mut value: Value = serde_json::from_slice(source_bytes).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!("Failed to parse registry.json: {error}"),
        )
    })?;
    let Some(object) = value.as_object_mut() else {
        return Err(Refusal::bad_registry(
            &registry.display().to_string(),
            "registry.json must be a JSON object",
        ));
    };
    object.insert(
        "version".to_string(),
        Value::String(version_after.to_string()),
    );
    object.insert(
        "entry_count".to_string(),
        Value::Number(serde_json::Number::from(entry_count_after)),
    );
    to_pretty_bytes(&value, registry)
}

pub(super) fn build_alias_bytes_with_entries(
    registry: &Path,
    alias_path: &Path,
    new_entries: &[RegistryAliasWriteEntry],
) -> Result<Vec<u8>, Refusal> {
    let bytes = fs::read(alias_path).map_err(|error| io_refusal(alias_path, error))?;
    build_alias_bytes_with_entries_from_source(registry, alias_path, &bytes, new_entries)
}

fn build_alias_bytes_from_source(
    registry: &Path,
    alias_path: &Path,
    source_bytes: &[u8],
    input: &str,
    canonical_id: &str,
    canonical_type: &str,
    rule_id: &str,
    scope: Option<&IdentityScope>,
) -> Result<Vec<u8>, Refusal> {
    let alias_file = alias_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let entry = RegistryAliasWriteEntry {
        alias_file,
        input: input.to_string(),
        canonical_id: canonical_id.to_string(),
        canonical_type: canonical_type.to_string(),
        rule_id: rule_id.to_string(),
        scope: scope.cloned(),
    };
    build_alias_bytes_with_entries_from_source(registry, alias_path, source_bytes, &[entry])
}

fn build_alias_bytes_with_entries_from_source(
    registry: &Path,
    alias_path: &Path,
    source_bytes: &[u8],
    new_entries: &[RegistryAliasWriteEntry],
) -> Result<Vec<u8>, Refusal> {
    let mut entries: Vec<Value> = serde_json::from_slice(source_bytes).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!(
                "Failed to parse alias file '{}': {error}",
                alias_path.display()
            ),
        )
    })?;
    for entry in new_entries {
        let mut alias = json!({
            "input": entry.input,
            "canonical_id": entry.canonical_id,
            "canonical_type": entry.canonical_type,
            "rule_id": entry.rule_id,
        });
        if let Some(scope) = &entry.scope {
            alias["scope"] = serde_json::to_value(scope).map_err(|error| {
                Refusal::bad_registry(
                    &registry.display().to_string(),
                    &format!("Failed to serialize proposed alias scope: {error}"),
                )
            })?;
        }
        entries.push(alias);
    }
    to_pretty_bytes(&entries, registry)
}

pub(super) fn to_pretty_bytes<T: Serialize + ?Sized>(
    value: &T,
    registry: &Path,
) -> Result<Vec<u8>, Refusal> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!("Failed to serialize proposed registry change: {error}"),
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn commit_add_entry_plan(
    mut plan: RegistryAddEntryPlan,
) -> Result<RegistryAddEntryOutput, Refusal> {
    let registry_dir = plan
        .registry_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let _guard = acquire_registry_mutation_guard(&registry_dir)
        .map_err(|error| io_refusal(&registry_dir, error))?;
    let alias_original =
        fs::read(&plan.alias_path).map_err(|error| io_refusal(&plan.alias_path, error))?;
    let registry_original =
        fs::read(&plan.registry_path).map_err(|error| io_refusal(&plan.registry_path, error))?;
    let planned_mutations = vec![
        PlannedFileMutation {
            path: plan.alias_path.clone(),
            expected_hash: plan.expected_alias_hash.clone(),
            proposed_hash: hash_bytes(&plan.alias_bytes),
        },
        PlannedFileMutation {
            path: plan.registry_path.clone(),
            expected_hash: plan.expected_registry_hash.clone(),
            proposed_hash: hash_bytes(&plan.registry_bytes),
        },
    ];
    match validate_planned_mutations(&planned_mutations)
        .map_err(|error| io_refusal(&registry_dir, error))?
    {
        PlannedMutationState::Ready => {}
        PlannedMutationState::AlreadyApplied => return Ok(plan.output),
        PlannedMutationState::Stale {
            path,
            expected_hash,
            actual_hash,
        } => {
            return Err(stale_write_plan_refusal(
                &registry_dir,
                &path,
                &expected_hash,
                &actual_hash,
            ));
        }
    }

    if let Err(error) = write_atomic(&plan.alias_path, &plan.alias_bytes) {
        return Err(io_refusal(&plan.alias_path, error));
    }
    if let Err(error) = write_atomic(&plan.registry_path, &plan.registry_bytes) {
        let _ = write_atomic(&plan.alias_path, &alias_original);
        return Err(io_refusal(&plan.registry_path, error));
    }

    if plan.lint_enabled {
        match crate::registry_lint::lint(
            plan.registry_path
                .parent()
                .unwrap_or_else(|| Path::new(".")),
            RegistryLintProfile::Standard,
        ) {
            Ok(lint) if lint.summary.errors == 0 => {
                plan.output.lint = lint_summary(&lint);
                plan.output.warnings = lint
                    .findings
                    .iter()
                    .filter(|finding| finding.severity != RegistryLintSeverity::Error)
                    .map(|finding| finding.code.clone())
                    .collect();
            }
            Ok(lint) => {
                restore_originals(&plan, &alias_original, &registry_original)?;
                return Err(bad_registry_refusal(
                    plan.registry_path
                        .parent()
                        .unwrap_or_else(|| Path::new(".")),
                    "Registry add-entry lint failed after proposed write",
                    json!({
                        "lint": lint,
                    }),
                    "Fix registry lint errors or rerun with --no-lint only after manual review",
                ));
            }
            Err(refusal) => {
                restore_originals(&plan, &alias_original, &registry_original)?;
                return Err(refusal);
            }
        }
    }

    Ok(plan.output)
}

pub(super) fn lint_summary(lint: &RegistryLintOutput) -> RegistryAddEntryLintSummary {
    RegistryAddEntryLintSummary {
        enabled: true,
        profile: Some(lint.profile.clone()),
        total_findings: lint.summary.total_findings,
        errors: lint.summary.errors,
        warnings: lint.summary.warnings,
        info: lint.summary.info,
    }
}

fn restore_originals(
    plan: &RegistryAddEntryPlan,
    alias_original: &[u8],
    registry_original: &[u8],
) -> Result<(), Refusal> {
    write_atomic(&plan.registry_path, registry_original)
        .map_err(|error| io_refusal(&plan.registry_path, error))?;
    write_atomic(&plan.alias_path, alias_original)
        .map_err(|error| io_refusal(&plan.alias_path, error))?;
    Ok(())
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    cleanup_stale_temp_siblings(path)?;
    let temp_path = temp_sibling(path);
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;
    if let Err(error) = temp_file.write_all(bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    if let Err(error) = temp_file.sync_all() {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    drop(temp_file);
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    Ok(())
}

fn temp_sibling(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("registry-file");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_file_name(format!(
        "{file_name}.canon-add-entry.{}.{}.tmp",
        std::process::id(),
        unique
    ))
}

fn cleanup_stale_temp_siblings(path: &Path) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("registry-file");
    let prefix = format!("{file_name}.canon-add-entry.");
    let now = SystemTime::now();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let entry_path = entry.path();
        let Some(name) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".tmp") {
            continue;
        }
        let age = entry
            .metadata()?
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or_default();
        if age >= TEMP_FILE_STALE_AFTER {
            let _ = fs::remove_file(&entry_path);
        }
    }
    Ok(())
}

pub(super) fn parse_refusal(
    registry: &Path,
    message: impl Into<String>,
    detail: Value,
    next_command: impl Into<String>,
) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: message.into(),
        detail: with_registry(registry, detail),
        next_command: Some(next_command.into()),
    }
}

pub(super) fn bad_registry_refusal(
    registry: &Path,
    message: impl Into<String>,
    detail: Value,
    next_command: impl Into<String>,
) -> Refusal {
    Refusal {
        code: RefusalCode::EBadRegistry,
        message: message.into(),
        detail: with_registry(registry, detail),
        next_command: Some(next_command.into()),
    }
}

fn with_registry(registry: &Path, mut detail: Value) -> Value {
    if let Some(object) = detail.as_object_mut() {
        object.insert(
            "registry".to_string(),
            Value::String(registry.display().to_string()),
        );
    }
    detail
}

pub(super) fn io_refusal(path: &Path, error: std::io::Error) -> Refusal {
    Refusal {
        code: RefusalCode::EIo,
        message: format!(
            "Unable to write registry path '{}': {error}",
            path.display()
        ),
        detail: json!({
            "path": path.display().to_string(),
            "error": error.to_string(),
        }),
        next_command: Some("Check paths and permissions, then rerun".to_string()),
    }
}

fn stale_write_plan_refusal(
    registry: &Path,
    path: &Path,
    expected_hash: &str,
    actual_hash: &str,
) -> Refusal {
    bad_registry_refusal(
        registry,
        "Registry mutation plan is stale relative to the current on-disk snapshot",
        json!({
            "field": "write_plan_hash",
            "path": path.display().to_string(),
            "expected_hash": expected_hash,
            "actual_hash": actual_hash,
            "writes_performed": false
        }),
        "Rebuild the registry mutation plan against the current files, then rerun",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;

    fn pretty_bytes(value: Value) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(&value).expect("serialize fixture");
        bytes.push(b'\n');
        bytes
    }

    fn make_registry() -> TempDir {
        let temp = TempDir::new().expect("temp registry");
        fs::write(
            temp.path().join("registry.json"),
            pretty_bytes(json!({
                "id": "people",
                "version": "1.0.0",
                "description": "add-entry plan fixture",
                "updated": "2026-07-11",
                "entry_count": 0
            })),
        )
        .expect("write registry metadata");
        fs::write(temp.path().join("aliases.json"), pretty_bytes(json!([])))
            .expect("write aliases");
        temp
    }

    fn add_entry_request(
        registry: &Path,
        canonical_id: &str,
        input: &str,
    ) -> RegistryAddEntryRequest {
        RegistryAddEntryRequest {
            registry: registry.to_path_buf(),
            alias_file: "aliases.json".to_string(),
            canonical_id: canonical_id.to_string(),
            input: input.to_string(),
            rule_id: "MANUAL".to_string(),
            canonical_type: Some("person".to_string()),
            bump: None,
            next_version: None,
            no_lint: true,
        }
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).expect("read JSON fixture"))
            .expect("parse JSON fixture")
    }

    #[test]
    fn stale_conflicting_add_entry_plan_refuses_without_lost_update() {
        let registry = make_registry();
        let first = plan_add_entry(add_entry_request(registry.path(), "PPL-001", "Alpha"))
            .expect("first plan");
        let second = plan_add_entry(add_entry_request(registry.path(), "PPL-002", "Beta"))
            .expect("second plan");

        commit_add_entry_plan(first).expect("first commit wins");
        let refusal = commit_add_entry_plan(second).expect_err("stale second plan refuses");

        assert_eq!(refusal.code, RefusalCode::EBadRegistry);
        assert_eq!(refusal.detail["field"], "write_plan_hash");
        assert_eq!(refusal.detail["writes_performed"], false);

        let aliases = read_json(&registry.path().join("aliases.json"));
        let entries = aliases.as_array().expect("aliases array");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["input"], "Alpha");
        assert_eq!(entries[0]["canonical_id"], "PPL-001");

        let registry_json = read_json(&registry.path().join("registry.json"));
        assert_eq!(registry_json["version"], "1.0.1");
        assert_eq!(registry_json["entry_count"], 1);
    }

    #[test]
    fn identical_precomputed_add_entry_plans_replay_without_duplication() {
        let registry = make_registry();
        let first = plan_add_entry(add_entry_request(registry.path(), "PPL-001", "Alpha"))
            .expect("first plan");
        let second = plan_add_entry(add_entry_request(registry.path(), "PPL-001", "Alpha"))
            .expect("second identical plan");

        let first_output = commit_add_entry_plan(first).expect("first commit");
        let replay_output = commit_add_entry_plan(second).expect("identical replay");
        assert_eq!(replay_output, first_output);

        let aliases = read_json(&registry.path().join("aliases.json"));
        let entries = aliases.as_array().expect("aliases array");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["input"], "Alpha");
        assert_eq!(entries[0]["canonical_id"], "PPL-001");

        let registry_json = read_json(&registry.path().join("registry.json"));
        assert_eq!(registry_json["version"], "1.0.1");
        assert_eq!(registry_json["entry_count"], 1);
    }
}
