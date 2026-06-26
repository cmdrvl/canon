#![forbid(unsafe_code)]

//! Promotion of reviewed entity aliases into the exact canon registry.
//!
//! Promotion is intentionally conservative: validate the matching passing audit
//! first, validate every alias against the current exact registry, then update
//! the alias file and registry metadata together. Registry lint failures restore
//! the original files.

use crate::{
    Refusal,
    entity::{
        audit::EntityAuditArtifact,
        contracts::{CANON_ENTITY_AUDIT_VERSION, CANON_ENTITY_PROMOTE_VERSION},
        error::EntityRefusalKind,
    },
    registry_lint::{self, RegistryLintOutput, RegistryLintProfile},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPromoteRegistryRequest {
    pub registry: PathBuf,
    pub alias_file: String,
    pub next_version: String,
    pub audit: EntityAuditArtifact,
    pub audit_expectation: EntityPromotionAuditExpectation,
    pub aliases: Vec<EntityPromotedAlias>,
    pub no_lint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPromotionAuditExpectation {
    pub audit_artifact_hash: String,
    pub audited_artifact_hash: String,
    pub profile_id: String,
    pub profile_version: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    #[serde(default)]
    pub required_gate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityPromotedAlias {
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPromoteRegistryOutput {
    pub version: String,
    pub audit_artifact_hash: String,
    pub registry: EntityPromoteRegistrySummary,
    pub aliases: Vec<EntityPromotedAlias>,
    pub touched_files: Vec<String>,
    pub lint: EntityPromoteLintSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPromoteRegistrySummary {
    pub id: String,
    pub version_before: String,
    pub version_after: String,
    pub entry_count_before: usize,
    pub entry_count_after: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPromoteLintSummary {
    pub enabled: bool,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryJson {
    id: String,
    version: String,
    entry_count: usize,
}

pub fn promote_registry_aliases(
    request: EntityPromoteRegistryRequest,
) -> Result<EntityPromoteRegistryOutput, Refusal> {
    validate_promotion_audit(&request.audit, &request.audit_expectation)?;
    let alias_path = validate_alias_file(&request.registry, &request.alias_file)?;
    let registry_path = request.registry.join("registry.json");
    let registry_original =
        fs::read(&registry_path).map_err(|error| io_refusal(&registry_path, error))?;
    let alias_original = fs::read(&alias_path).map_err(|error| io_refusal(&alias_path, error))?;

    let (registry_json, registry_value) =
        parse_registry_json(&request.registry, &registry_original)?;
    let aliases = validate_aliases(&request.registry, &alias_path, &request.aliases)?;
    let requested_version = validate_requested_version(&request.next_version)?;
    let existing_aliases = parse_alias_entries(&request.registry, &alias_path, &alias_original)?;
    if registry_json.version == requested_version
        && aliases_already_promoted(&existing_aliases, &aliases)
    {
        let lint = lint_current_registry(&request.registry, request.no_lint)?;
        return Ok(EntityPromoteRegistryOutput {
            version: CANON_ENTITY_PROMOTE_VERSION.to_string(),
            audit_artifact_hash: request.audit.artifact_content_hash,
            registry: EntityPromoteRegistrySummary {
                id: registry_json.id,
                version_before: registry_json.version.clone(),
                version_after: registry_json.version,
                entry_count_before: registry_json.entry_count,
                entry_count_after: registry_json.entry_count,
            },
            aliases,
            touched_files: vec![],
            lint,
        });
    }
    let next_version = validate_next_version(&registry_json.version, &requested_version)?;
    let existing_inputs = existing_registry_inputs(&request.registry)?;
    for alias in &aliases {
        if existing_inputs.contains(alias.input.as_str()) {
            return Err(promote_refusal(
                "Promotion alias already exists in the registry",
                json!({
                    "stage": "promote",
                    "field": "input",
                    "input": alias.input,
                    "writes_performed": false
                }),
            ));
        }
    }

    let entry_count_after = registry_json
        .entry_count
        .checked_add(aliases.len())
        .ok_or_else(|| {
            promote_refusal(
                "Registry entry_count is too large to increment",
                json!({
                    "stage": "promote",
                    "entry_count": registry_json.entry_count,
                    "aliases_to_add": aliases.len(),
                    "writes_performed": false
                }),
            )
        })?;
    let registry_bytes = build_registry_bytes(registry_value, &next_version, entry_count_after)?;
    let alias_bytes = build_alias_bytes(&alias_original, &aliases)?;

    write_atomic(&alias_path, &alias_bytes).map_err(|error| io_refusal(&alias_path, error))?;
    if let Err(error) = write_atomic(&registry_path, &registry_bytes) {
        let _ = write_atomic(&alias_path, &alias_original);
        return Err(io_refusal(&registry_path, error));
    }

    let lint = if request.no_lint {
        EntityPromoteLintSummary {
            enabled: false,
            errors: 0,
            warnings: 0,
            info: 0,
        }
    } else {
        match registry_lint::lint(&request.registry, RegistryLintProfile::Standard) {
            Ok(output) if output.summary.errors == 0 => lint_summary(&output),
            Ok(output) => {
                restore_originals(
                    &registry_path,
                    &registry_original,
                    &alias_path,
                    &alias_original,
                )?;
                return Err(Refusal::bad_registry(
                    &request.registry.display().to_string(),
                    &format!(
                        "Registry lint failed after promotion: {}",
                        output.render_summary()
                    ),
                ));
            }
            Err(refusal) => {
                restore_originals(
                    &registry_path,
                    &registry_original,
                    &alias_path,
                    &alias_original,
                )?;
                return Err(refusal);
            }
        }
    };

    Ok(EntityPromoteRegistryOutput {
        version: CANON_ENTITY_PROMOTE_VERSION.to_string(),
        audit_artifact_hash: request.audit.artifact_content_hash,
        registry: EntityPromoteRegistrySummary {
            id: registry_json.id,
            version_before: registry_json.version,
            version_after: next_version,
            entry_count_before: registry_json.entry_count,
            entry_count_after,
        },
        aliases,
        touched_files: vec![request.alias_file, "registry.json".to_string()],
        lint,
    })
}

fn validate_promotion_audit(
    audit: &EntityAuditArtifact,
    expected: &EntityPromotionAuditExpectation,
) -> Result<(), Refusal> {
    if audit.version != CANON_ENTITY_AUDIT_VERSION {
        return Err(audit_gate_refusal(
            "audit_version",
            CANON_ENTITY_AUDIT_VERSION,
            &audit.version,
        ));
    }
    compare_audit_gate_field(
        "audit_artifact_hash",
        &expected.audit_artifact_hash,
        &audit.artifact_content_hash,
    )?;
    compare_audit_gate_field(
        "audited_artifact_hash",
        &expected.audited_artifact_hash,
        &audit.audited_artifact.content_hash,
    )?;
    compare_audit_gate_field(
        "profile_id",
        &expected.profile_id,
        &audit.metadata.profile.id,
    )?;
    compare_audit_gate_field(
        "profile_version",
        &expected.profile_version,
        &audit.metadata.profile.version,
    )?;
    compare_audit_gate_field(
        "strategy_hash",
        &expected.strategy_hash,
        &audit.metadata.strategy.content_hash,
    )?;
    if expected.registry_snapshot_hash != audit.metadata.registry_snapshot.lookup_snapshot_hash {
        return Err(EntityRefusalKind::RegistrySnapshot.to_refusal(
            "Promotion audit registry snapshot does not match the target registry snapshot",
            json!({
                "stage": "promote",
                "field": "registry_snapshot_hash",
                "expected": expected.registry_snapshot_hash,
                "actual": audit.metadata.registry_snapshot.lookup_snapshot_hash,
                "writes_performed": false
            }),
            Some("Use the matching registry snapshot or rerun canon entity audit".to_string()),
        ));
    }
    if audit.summary.labels.get("status").map(String::as_str) != Some("passed") {
        return Err(audit_gate_refusal(
            "audit_status",
            "passed",
            audit
                .summary
                .labels
                .get("status")
                .map(String::as_str)
                .unwrap_or("<missing>"),
        ));
    }
    let passed_gate_ids = audit
        .gates
        .iter()
        .map(|gate| gate.gate_id.as_str())
        .collect::<BTreeSet<_>>();
    for required in &expected.required_gate_ids {
        if !passed_gate_ids.contains(required.as_str()) {
            return Err(audit_gate_refusal(
                "required_gate_id",
                required,
                "<missing>",
            ));
        }
    }
    Ok(())
}

fn compare_audit_gate_field(field: &str, expected: &str, actual: &str) -> Result<(), Refusal> {
    if expected == actual {
        Ok(())
    } else {
        Err(audit_gate_refusal(field, expected, actual))
    }
}

fn audit_gate_refusal(field: &str, expected: &str, actual: &str) -> Refusal {
    EntityRefusalKind::AuditGate.to_refusal(
        "Promotion requires a matching passing audit artifact",
        json!({
            "stage": "promote",
            "field": field,
            "expected": expected,
            "actual": actual,
            "writes_performed": false
        }),
        Some("Rerun canon entity audit for the exact result and registry snapshot".to_string()),
    )
}

fn validate_alias_file(registry: &Path, alias_file: &str) -> Result<PathBuf, Refusal> {
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
        return Err(promote_refusal(
            "Invalid promotion alias file",
            json!({
                "stage": "promote",
                "field": "alias_file",
                "alias_file": alias_file,
                "writes_performed": false
            }),
        ));
    }
    let full_path = registry.join(alias_file);
    if !full_path.is_file() {
        return Err(promote_refusal(
            "Promotion alias file must already exist",
            json!({
                "stage": "promote",
                "field": "alias_file",
                "alias_file": alias_file,
                "writes_performed": false
            }),
        ));
    }
    Ok(full_path)
}

fn parse_registry_json(registry: &Path, bytes: &[u8]) -> Result<(RegistryJson, Value), Refusal> {
    let value = serde_json::from_slice::<Value>(bytes).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!("Failed to parse registry.json: {error}"),
        )
    })?;
    let parsed = serde_json::from_value::<RegistryJson>(value.clone()).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!("registry.json is missing required promotion fields: {error}"),
        )
    })?;
    Ok((parsed, value))
}

fn validate_requested_version(next: &str) -> Result<String, Refusal> {
    let trimmed = ascii_trim(next);
    if trimmed.is_empty() {
        return Err(promote_refusal(
            "Promotion requires an explicit new registry version",
            json!({
                "stage": "promote",
                "field": "next_version",
                "next_version": next,
                "writes_performed": false
            }),
        ));
    }
    if trimmed != next {
        return Err(promote_refusal(
            "Promotion next_version must already be ASCII-trimmed",
            json!({
                "stage": "promote",
                "field": "next_version",
                "next_version": next,
                "trimmed": trimmed,
                "writes_performed": false
            }),
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_next_version(current: &str, next: &str) -> Result<String, Refusal> {
    let trimmed = ascii_trim(next);
    if trimmed.is_empty() || trimmed == current {
        return Err(promote_refusal(
            "Promotion requires an explicit new registry version",
            json!({
                "stage": "promote",
                "field": "next_version",
                "current_version": current,
                "next_version": next,
                "writes_performed": false
            }),
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_aliases(
    registry: &Path,
    alias_path: &Path,
    aliases: &[EntityPromotedAlias],
) -> Result<Vec<EntityPromotedAlias>, Refusal> {
    if aliases.is_empty() {
        return Err(promote_refusal(
            "Promotion requires at least one alias",
            json!({
                "stage": "promote",
                "field": "aliases",
                "writes_performed": false
            }),
        ));
    }
    let mut seen_inputs = BTreeSet::new();
    let mut validated = aliases.to_vec();
    for alias in &mut validated {
        alias.input = validate_trimmed(registry, "input", &alias.input, true)?;
        alias.canonical_id =
            validate_trimmed(registry, "canonical_id", &alias.canonical_id, false)?;
        alias.canonical_type =
            validate_trimmed(registry, "canonical_type", &alias.canonical_type, false)?;
        alias.rule_id = validate_trimmed(registry, "rule_id", &alias.rule_id, false)?;
        if !seen_inputs.insert(alias.input.clone()) {
            return Err(promote_refusal(
                "Promotion contains duplicate alias inputs",
                json!({
                    "stage": "promote",
                    "field": "input",
                    "input": alias.input,
                    "writes_performed": false
                }),
            ));
        }
    }
    let _ = alias_path;
    validated.sort();
    Ok(validated)
}

fn validate_trimmed(
    _registry: &Path,
    field: &str,
    value: &str,
    require_already_trimmed: bool,
) -> Result<String, Refusal> {
    let trimmed = ascii_trim(value);
    if trimmed.is_empty() {
        return Err(promote_refusal(
            "Promotion alias field must not be empty",
            json!({
                "stage": "promote",
                "field": field,
                "writes_performed": false
            }),
        ));
    }
    if require_already_trimmed && trimmed != value {
        return Err(promote_refusal(
            "Promotion alias input must already be ASCII-trimmed",
            json!({
                "stage": "promote",
                "field": field,
                "input": value,
                "trimmed": trimmed,
                "writes_performed": false
            }),
        ));
    }
    Ok(trimmed.to_string())
}

fn existing_registry_inputs(registry: &Path) -> Result<BTreeSet<String>, Refusal> {
    let mut inputs = BTreeSet::new();
    for path in registry_json_files(registry)? {
        let bytes = fs::read(&path).map_err(|error| io_refusal(&path, error))?;
        let entries = parse_alias_entries(registry, &path, &bytes)?;
        for entry in entries {
            inputs.insert(entry.input);
        }
    }
    Ok(inputs)
}

fn registry_json_files(registry: &Path) -> Result<Vec<PathBuf>, Refusal> {
    let mut files = Vec::new();
    let entries = fs::read_dir(registry).map_err(|error| io_refusal(registry, error))?;
    for entry in entries {
        let path = entry.map_err(|error| io_refusal(registry, error))?.path();
        if path.file_name() == Some(OsStr::new("registry.json"))
            || path.file_name() == Some(OsStr::new("_build.json"))
            || !path
                .extension()
                .is_some_and(|extension| extension == "json")
        {
            continue;
        }
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct RegistryAliasEntry {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

fn parse_alias_entries(
    registry: &Path,
    path: &Path,
    bytes: &[u8],
) -> Result<Vec<RegistryAliasEntry>, Refusal> {
    serde_json::from_slice::<Vec<RegistryAliasEntry>>(bytes).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!("Failed to parse mapping file '{}': {error}", path.display()),
        )
    })
}

fn aliases_already_promoted(
    existing_aliases: &[RegistryAliasEntry],
    aliases: &[EntityPromotedAlias],
) -> bool {
    let existing = existing_aliases.iter().cloned().collect::<BTreeSet<_>>();
    aliases
        .iter()
        .map(RegistryAliasEntry::from)
        .all(|alias| existing.contains(&alias))
}

impl From<&EntityPromotedAlias> for RegistryAliasEntry {
    fn from(alias: &EntityPromotedAlias) -> Self {
        Self {
            input: alias.input.clone(),
            canonical_id: alias.canonical_id.clone(),
            canonical_type: alias.canonical_type.clone(),
            rule_id: alias.rule_id.clone(),
        }
    }
}

fn build_registry_bytes(
    mut registry_value: Value,
    next_version: &str,
    entry_count_after: usize,
) -> Result<Vec<u8>, Refusal> {
    let Some(object) = registry_value.as_object_mut() else {
        return Err(Refusal::bad_registry(
            "registry.json",
            "registry.json must be a JSON object",
        ));
    };
    object.insert(
        "version".to_string(),
        Value::String(next_version.to_string()),
    );
    object.insert(
        "entry_count".to_string(),
        Value::Number(serde_json::Number::from(entry_count_after)),
    );
    to_pretty_bytes(&registry_value)
}

fn build_alias_bytes(
    alias_original: &[u8],
    aliases: &[EntityPromotedAlias],
) -> Result<Vec<u8>, Refusal> {
    let mut entries = serde_json::from_slice::<Vec<Value>>(alias_original).map_err(|error| {
        Refusal::bad_registry(
            "alias file",
            &format!("Failed to parse promotion alias file: {error}"),
        )
    })?;
    for alias in aliases {
        entries.push(json!({
            "input": alias.input,
            "canonical_id": alias.canonical_id,
            "canonical_type": alias.canonical_type,
            "rule_id": alias.rule_id,
        }));
    }
    to_pretty_bytes(&entries)
}

fn to_pretty_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Refusal> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        promote_refusal(
            "Failed to serialize promotion registry update",
            json!({
                "stage": "promote",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn lint_summary(output: &RegistryLintOutput) -> EntityPromoteLintSummary {
    EntityPromoteLintSummary {
        enabled: true,
        errors: output.summary.errors,
        warnings: output.summary.warnings,
        info: output.summary.info,
    }
}

fn lint_current_registry(
    registry: &Path,
    no_lint: bool,
) -> Result<EntityPromoteLintSummary, Refusal> {
    if no_lint {
        return Ok(EntityPromoteLintSummary {
            enabled: false,
            errors: 0,
            warnings: 0,
            info: 0,
        });
    }
    match registry_lint::lint(registry, RegistryLintProfile::Standard) {
        Ok(output) if output.summary.errors == 0 => Ok(lint_summary(&output)),
        Ok(output) => Err(Refusal::bad_registry(
            &registry.display().to_string(),
            &format!(
                "Registry lint failed during idempotent promotion replay: {}",
                output.render_summary()
            ),
        )),
        Err(refusal) => Err(refusal),
    }
}

fn restore_originals(
    registry_path: &Path,
    registry_original: &[u8],
    alias_path: &Path,
    alias_original: &[u8],
) -> Result<(), Refusal> {
    write_atomic(registry_path, registry_original)
        .map_err(|error| io_refusal(registry_path, error))?;
    write_atomic(alias_path, alias_original).map_err(|error| io_refusal(alias_path, error))?;
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
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
    path.with_file_name(format!("{file_name}.canon-promote.tmp"))
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

fn promote_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some(
            "canon entity promote <RESULT.json> --audit <AUDIT.json> --registry <REGISTRY_DIR>"
                .to_string(),
        ),
    )
}

fn io_refusal(path: &Path, error: std::io::Error) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        format!("Entity promotion could not access {}", path.display()),
        json!({
            "stage": "promote",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        Some("Check file permissions and rerun canon entity promote".to_string()),
    )
}
