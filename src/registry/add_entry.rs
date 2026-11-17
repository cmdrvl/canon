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
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryAddEntryAliasEntry {
    pub alias_file: String,
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_authority: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RegistryAliasWriteEntry {
    pub alias_file: String,
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
    pub scope: Option<IdentityScope>,
    /// Identifier namespace recorded on the entry (key aliases only).
    pub namespace: Option<crate::identity_scope::IdentifierNamespaceRef>,
    pub session_authority: Option<Value>,
}

impl RegistryAliasWriteEntry {
    pub(super) fn output_entry(&self) -> RegistryAddEntryAliasEntry {
        RegistryAddEntryAliasEntry {
            alias_file: self.alias_file.clone(),
            input: self.input.clone(),
            canonical_id: self.canonical_id.clone(),
            canonical_type: self.canonical_type.clone(),
            rule_id: self.rule_id.clone(),
            session_authority: self.session_authority.clone(),
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

/// One existing canonical ID and one exact alias to attach to it.  The JSONL
/// form deliberately omits `canonical_type`: it is read from the incumbent
/// registry entry so a batch cannot change an entity's kind while accreting.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryAddEntriesItem {
    pub canonical_id: String,
    pub alias_file: String,
    pub input: String,
    pub rule_id: String,
}

#[derive(Debug, Clone)]
pub struct RegistryAddEntriesRequest {
    pub registry: PathBuf,
    pub items: Vec<RegistryAddEntriesItem>,
    pub bump: Option<RegistryVersionBump>,
    pub next_version: Option<String>,
    pub no_lint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryAddEntriesOutput {
    pub version: String,
    pub registry: RegistryAddEntryRegistry,
    pub attached: Vec<RegistryAddEntryAliasEntry>,
    pub skipped: Vec<String>,
    pub touched_files: Vec<String>,
    pub lint: RegistryAddEntryLintSummary,
    pub warnings: Vec<String>,
}

impl RegistryAddEntriesOutput {
    pub fn render_plain(&self) -> String {
        self.attached
            .iter()
            .map(|entry| format!("{} -> {}", entry.input, entry.canonical_id))
            .collect::<Vec<_>>()
            .join("\n")
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
    let alias_entry = RegistryAliasWriteEntry {
        alias_file: alias_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        input: input.clone(),
        canonical_id: canonical_id.clone(),
        canonical_type: canonical_type.clone(),
        rule_id: rule_id.clone(),
        scope: scope.clone(),
        namespace: None,
        session_authority: None,
    };
    let alias_bytes = build_alias_bytes_with_entries_from_source(
        &request.registry,
        &alias_path,
        &alias_source_bytes,
        &[alias_entry],
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
            session_authority: None,
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
    validate_alias_file_allowing_missing(registry, alias_file, false)
}

/// Same name and path rules as [`validate_alias_file`]. With `allow_missing`,
/// an alias file that does not exist yet is accepted so the caller can create
/// it transactionally; anything else at that path (directory, symlink,
/// dangling symlink) is still refused.
pub(super) fn validate_alias_file_allowing_missing(
    registry: &Path,
    alias_file: &str,
    allow_missing: bool,
) -> Result<PathBuf, Refusal> {
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
    let missing = matches!(
        fs::symlink_metadata(&full_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    if missing && allow_missing {
        return Ok(full_path);
    }
    if !full_path.is_file() {
        return Err(parse_refusal(
            registry,
            "Registry add-entry alias file must already exist",
            json!({
                "alias_file": alias_file,
                "path": full_path.display().to_string(),
            }),
            "Create an empty root alias file such as [] before rerunning, or pass --create-alias-file to registry mint",
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
    stamp_updated(object);
    to_pretty_bytes(&value, registry)
}

/// `updated` is the date of the last registry mutation, as `registry build`
/// already stamps it; a version bump that leaves it behind misdates the
/// registry. Callers reach this only on a successful mutation, so refusals
/// leave the metadata untouched.
pub(super) fn stamp_updated(object: &mut serde_json::Map<String, Value>) {
    object.insert(
        "updated".to_string(),
        Value::String(chrono::Utc::now().format("%Y-%m-%d").to_string()),
    );
}

pub(super) fn build_alias_bytes_with_entries(
    registry: &Path,
    alias_path: &Path,
    new_entries: &[RegistryAliasWriteEntry],
) -> Result<Vec<u8>, Refusal> {
    let bytes = fs::read(alias_path).map_err(|error| io_refusal(alias_path, error))?;
    build_alias_bytes_with_entries_from_source(registry, alias_path, &bytes, new_entries)
}

/// Like [`build_alias_bytes_with_entries`], but an alias file that does not
/// exist yet starts from an empty `[]` list (the file `mint` then creates).
pub(super) fn build_alias_bytes_creating_if_missing(
    registry: &Path,
    alias_path: &Path,
    new_entries: &[RegistryAliasWriteEntry],
) -> Result<Vec<u8>, Refusal> {
    match fs::symlink_metadata(alias_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            build_alias_bytes_with_entries_from_source(registry, alias_path, b"[]", new_entries)
        }
        _ => build_alias_bytes_with_entries(registry, alias_path, new_entries),
    }
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
        if let Some(session_authority) = &entry.session_authority {
            alias["session_authority"] = session_authority.clone();
        }
        if let Some(namespace) = &entry.namespace {
            alias["namespace"] = serde_json::to_value(namespace).map_err(|error| {
                Refusal::bad_registry(
                    &registry.display().to_string(),
                    &format!("Failed to serialize proposed alias namespace: {error}"),
                )
            })?;
        }
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

/// Parse one JSON object per line for `registry add-entries`. Parsing all
/// lines happens before the typed writer is called, so a malformed later line
/// cannot leave an earlier alias on disk.
pub fn read_add_entries(
    registry: &Path,
    path: &Path,
) -> Result<Vec<RegistryAddEntriesItem>, Refusal> {
    let text = fs::read_to_string(path).map_err(|error| io_refusal(path, error))?;
    let mut items = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let item = serde_json::from_str::<RegistryAddEntriesItem>(line).map_err(|error| {
            parse_refusal(
                registry,
                "Invalid add-entries line; expected {\"canonical_id\": ..., \"alias_file\": ..., \"input\": ..., \"rule_id\": ...}",
                json!({ "line": line_index + 1, "error": error.to_string() }),
                "Fix the add-entries JSONL file, then rerun; nothing was written",
            )
        })?;
        items.push(item);
    }
    Ok(items)
}

/// Attach a batch of exact aliases to incumbent canonical IDs in one planned
/// registry write. Existing aliases of the same ID are idempotent no-ops;
/// an alias already owned by another ID refuses the whole batch.
pub fn add_entries(
    request: RegistryAddEntriesRequest,
) -> Result<RegistryAddEntriesOutput, Refusal> {
    if request.items.is_empty() {
        return Err(parse_refusal(
            &request.registry,
            "canon registry add-entries requires at least one JSONL item",
            json!({}),
            "Add one canonical_id/alias_file/input/rule_id object per line, then rerun",
        ));
    }

    let (registry_json, _, mapping_files) =
        load_registry_definition(&request.registry).map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;
    let mut seen_inputs = BTreeSet::new();
    let mut typed_entries = Vec::with_capacity(request.items.len());
    for item in request.items {
        let canonical_id = validate_trimmed_non_empty(
            &request.registry,
            "canonical_id",
            &item.canonical_id,
            false,
            "Use a non-empty canonical_id",
        )?;
        if !mapping_files.iter().any(|file| {
            file.entries
                .iter()
                .any(|entry| entry.canonical_id == canonical_id)
        }) {
            return Err(parse_refusal(
                &request.registry,
                "Unknown canonical_id in add-entries batch",
                json!({ "canonical_id": canonical_id, "writes_performed": false }),
                "Use a canonical_id already present in the registry, then rerun",
            ));
        }
        let canonical_type =
            resolve_canonical_type(&request.registry, &canonical_id, None, &mapping_files)?;
        let alias_path = validate_alias_file(&request.registry, &item.alias_file)?;
        let alias_file = alias_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(item.alias_file.as_str())
            .to_string();
        let input = validate_trimmed_non_empty(
            &request.registry,
            "input",
            &item.input,
            true,
            "Use an ASCII-trimmed input",
        )?;
        if !seen_inputs.insert(input.clone()) {
            return Err(parse_refusal(
                &request.registry,
                "Duplicate input inside add-entries batch",
                json!({ "input": input, "writes_performed": false }),
                "Remove the duplicate input, then rerun; nothing was written",
            ));
        }
        if let Some(existing) = mapping_files
            .iter()
            .flat_map(|file| file.entries.iter())
            .find(|entry| entry.input == input)
            && existing.canonical_id != canonical_id
        {
            return Err(parse_refusal(
                &request.registry,
                "Input alias already belongs to a different canonical_id",
                json!({
                    "input": input,
                    "requested_canonical_id": canonical_id,
                    "existing_canonical_id": existing.canonical_id,
                    "writes_performed": false,
                }),
                "Choose an unowned input or resolve the incumbent conflict before rerunning",
            ));
        }
        let rule_id = validate_trimmed_non_empty(
            &request.registry,
            "rule_id",
            &item.rule_id,
            false,
            "Use a non-empty rule_id",
        )?;
        typed_entries.push(super::mint::RegistryTypedAlias {
            alias_file,
            input,
            canonical_id: Some(canonical_id),
            new_id_slot: None,
            canonical_type,
            rule_id,
            skip_if_existing: true,
            session_authority: None,
            scope: None,
            founding_ref: None,
        });
    }

    let typed = super::mint::commit_typed_entries(super::mint::RegistryTypedWriteRequest {
        registry: request.registry,
        prefix: None,
        zero_pad: None,
        entries: typed_entries,
        bump: request.bump,
        next_version: request.next_version,
        no_lint: request.no_lint,
        expected_version: Some(registry_json.version),
        agent_ids: None,
    })?;
    let attached = typed
        .minted
        .iter()
        .flat_map(|entry| entry.aliases.iter().cloned())
        .collect::<Vec<_>>();
    let skipped = typed
        .skipped
        .into_iter()
        .map(|entry| format!("{}: {}", entry.input, entry.reason))
        .collect::<Vec<_>>();
    Ok(RegistryAddEntriesOutput {
        version: "canon_registry_add_entries.v0".to_string(),
        registry: typed.registry,
        attached,
        skipped,
        touched_files: typed.touched_files,
        lint: typed.lint,
        warnings: typed.warnings,
    })
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
    fn add_entries_groups_incumbent_aliases_and_rejects_unknown_ids_before_write() {
        let registry = make_registry();
        add_entry(add_entry_request(registry.path(), "PPL-001", "Alpha"))
            .expect("seed incumbent through registry writer");
        let output = add_entries(RegistryAddEntriesRequest {
            registry: registry.path().to_path_buf(),
            items: vec![
                RegistryAddEntriesItem {
                    canonical_id: "PPL-001".to_string(),
                    alias_file: "aliases.json".to_string(),
                    input: "Alpha Variant".to_string(),
                    rule_id: "SESSION_TIER_A".to_string(),
                },
                RegistryAddEntriesItem {
                    canonical_id: "PPL-001".to_string(),
                    alias_file: "aliases.json".to_string(),
                    input: "Alpha".to_string(),
                    rule_id: "SESSION_TIER_A".to_string(),
                },
            ],
            bump: None,
            next_version: None,
            no_lint: true,
        })
        .expect("attach incumbent aliases");
        assert_eq!(output.attached.len(), 1);
        assert_eq!(output.skipped, vec!["Alpha: already_present"]);
        assert_eq!(output.registry.version_after, "1.0.2");
        assert_eq!(
            read_json(&registry.path().join("registry.json"))["entry_count"],
            2
        );

        let before = read_json(&registry.path().join("registry.json"));
        let refusal = add_entries(RegistryAddEntriesRequest {
            registry: registry.path().to_path_buf(),
            items: vec![RegistryAddEntriesItem {
                canonical_id: "PPL-999".to_string(),
                alias_file: "aliases.json".to_string(),
                input: "Should Not Write".to_string(),
                rule_id: "SESSION_TIER_A".to_string(),
            }],
            bump: None,
            next_version: None,
            no_lint: true,
        })
        .expect_err("unknown incumbent id must refuse");
        assert_eq!(refusal.code, RefusalCode::EParse);
        assert_eq!(read_json(&registry.path().join("registry.json")), before);
        assert!(
            !read_json(&registry.path().join("aliases.json"))
                .as_array()
                .expect("aliases")
                .iter()
                .any(|entry| entry["input"] == "Should Not Write")
        );
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
