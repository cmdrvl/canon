use super::{DefaultIdScheme, MappingFile, load_registry_definition};
use crate::{
    Refusal, RefusalCode, RegistryMeta,
    registry_lint::{RegistryLintOutput, RegistryLintProfile, RegistryLintSeverity},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

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
    pub registry_bytes: Vec<u8>,
    pub alias_bytes: Vec<u8>,
    pub lint_enabled: bool,
    pub output: RegistryAddEntryOutput,
}

pub fn add_entry(request: RegistryAddEntryRequest) -> Result<RegistryAddEntryOutput, Refusal> {
    let plan = plan_add_entry(request)?;
    commit_add_entry_plan(plan)
}

pub fn plan_add_entry(request: RegistryAddEntryRequest) -> Result<RegistryAddEntryPlan, Refusal> {
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
    ensure_input_is_new(&request.registry, &input, &mapping_files)?;
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
    let registry_bytes = build_registry_bytes(
        &request.registry,
        &registry_path,
        &version_after,
        entry_count_after,
    )?;
    let alias_bytes = build_alias_bytes(
        &request.registry,
        &alias_path,
        &input,
        &canonical_id,
        &canonical_type,
        &rule_id,
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
        registry_bytes,
        alias_bytes,
        lint_enabled: !request.no_lint,
        output,
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

pub(super) fn ensure_input_is_new(
    registry: &Path,
    input: &str,
    mapping_files: &[MappingFile],
) -> Result<(), Refusal> {
    for mapping_file in mapping_files {
        for entry in &mapping_file.entries {
            if entry.input == input {
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
                        }
                    }),
                    "Choose a new input alias or edit the existing mapping intentionally",
                ));
            }
        }
    }
    Ok(())
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
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|error| {
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

fn build_alias_bytes(
    registry: &Path,
    alias_path: &Path,
    input: &str,
    canonical_id: &str,
    canonical_type: &str,
    rule_id: &str,
) -> Result<Vec<u8>, Refusal> {
    let alias_file = alias_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let entry = RegistryAddEntryAliasEntry {
        alias_file,
        input: input.to_string(),
        canonical_id: canonical_id.to_string(),
        canonical_type: canonical_type.to_string(),
        rule_id: rule_id.to_string(),
    };
    build_alias_bytes_with_entries(registry, alias_path, &[entry])
}

pub(super) fn build_alias_bytes_with_entries(
    registry: &Path,
    alias_path: &Path,
    new_entries: &[RegistryAddEntryAliasEntry],
) -> Result<Vec<u8>, Refusal> {
    let bytes = fs::read(alias_path).map_err(|error| io_refusal(alias_path, error))?;
    let mut entries: Vec<Value> = serde_json::from_slice(&bytes).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!(
                "Failed to parse alias file '{}': {error}",
                alias_path.display()
            ),
        )
    })?;
    for entry in new_entries {
        entries.push(json!({
            "input": entry.input,
            "canonical_id": entry.canonical_id,
            "canonical_type": entry.canonical_type,
            "rule_id": entry.rule_id,
        }));
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
    let alias_original =
        fs::read(&plan.alias_path).map_err(|error| io_refusal(&plan.alias_path, error))?;
    let registry_original =
        fs::read(&plan.registry_path).map_err(|error| io_refusal(&plan.registry_path, error))?;

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
    let temp_path = temp_sibling(path);
    if temp_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("temporary path already exists: {}", temp_path.display()),
        ));
    }
    if let Err(error) = fs::write(&temp_path, bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
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
    path.with_file_name(format!("{file_name}.canon-add-entry.tmp"))
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
