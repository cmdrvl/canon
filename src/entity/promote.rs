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
        contracts::{
            CANON_ENTITY_AUDIT_VERSION, CANON_ENTITY_AUDIT_VERSION_V1,
            CANON_ENTITY_PROMOTE_VERSION, CANON_ENTITY_PROMOTE_VERSION_V1,
            CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactReference,
            EntityArtifactStageV1, EntityProfileReference,
        },
        error::EntityRefusalKind,
        review::{required_value_string, value_string_or, value_u64_or},
        run::{
            EntityRunArtifact,
            link::{
                EntityLinkArtifact, LINK_ARTIFACT_PATH, validate_entity_link_artifact_contract,
                validate_entity_link_artifact_raw_shape,
            },
            read_entity_run_committed_publication_logical_bytes,
        },
        schema::{
            entity_v1_artifact_reference, entity_v1_lifecycle_metadata_from_source,
            finalize_entity_v1_self_hash, validate_artifact_v1_core_contract,
            validate_entity_v1_self_hash,
        },
        solve::{SolveAliasProposal, SolveArtifact, validate_solve_artifact_envelope_contract},
    },
    registry::{
        PlannedMutationState, acquire_registry_mutation_guard, planned_file_mutation,
        validate_planned_mutations,
    },
    registry_lint::{self, RegistryLintOutput, RegistryLintProfile},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TEMP_FILE_STALE_AFTER: Duration = Duration::from_secs(30);
const RUN_ARTIFACT_PATH: &str = "run/run.json";

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
    validate_registry_target(&registry_json, &request.audit)?;
    validate_existing_profile_metadata(&registry_value, &request.audit.metadata.profile)?;
    let aliases = validate_aliases(&request.registry, &alias_path, &request.aliases)?;
    let requested_version = validate_requested_version(&request.next_version)?;
    let existing_aliases = parse_alias_entries(&request.registry, &alias_path, &alias_original)?;
    if registry_json.version == requested_version
        && aliases_already_promoted(&existing_aliases, &aliases)
    {
        if !registry_profile_metadata_matches(&registry_value, &request.audit.metadata.profile) {
            return Err(registry_profile_refusal(
                "entity_profile",
                "<audited profile metadata>",
                "<missing>",
            ));
        }
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
    let registry_bytes = build_registry_bytes(
        registry_value,
        &next_version,
        entry_count_after,
        &request.audit.metadata.profile,
    )?;
    let alias_bytes = build_alias_bytes(&alias_original, &aliases)?;
    let planned_mutations = vec![
        planned_file_mutation(&alias_path, &alias_original, &alias_bytes),
        planned_file_mutation(&registry_path, &registry_original, &registry_bytes),
    ];
    let _guard = acquire_registry_mutation_guard(&request.registry)
        .map_err(|error| io_refusal(&request.registry, error))?;
    match validate_planned_mutations(&planned_mutations)
        .map_err(|error| io_refusal(&request.registry, error))?
    {
        PlannedMutationState::Ready => {}
        PlannedMutationState::AlreadyApplied => {
            let lint = lint_current_registry(&request.registry, request.no_lint)?;
            return Ok(EntityPromoteRegistryOutput {
                version: CANON_ENTITY_PROMOTE_VERSION.to_string(),
                audit_artifact_hash: request.audit.artifact_content_hash,
                registry: EntityPromoteRegistrySummary {
                    id: registry_json.id,
                    version_before: registry_json.version.clone(),
                    version_after: next_version,
                    entry_count_before: registry_json.entry_count,
                    entry_count_after,
                },
                aliases,
                touched_files: vec![],
                lint,
            });
        }
        PlannedMutationState::Stale {
            path,
            expected_hash,
            actual_hash,
        } => {
            return Err(stale_registry_snapshot_refusal(
                &path,
                &expected_hash,
                &actual_hash,
            ));
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPromoteV1Request {
    pub result_path: PathBuf,
    pub result_artifact: Value,
    pub audit_artifact: Value,
    pub registry: PathBuf,
    pub next_version: String,
}

pub fn promote_entity_v1(request: EntityPromoteV1Request) -> Result<Value, Refusal> {
    validate_promote_v1_source(&request.result_artifact)?;
    validate_promote_v1_audit(&request.result_artifact, &request.audit_artifact)?;
    refuse_link_bound_promotion(&request)?;
    let aliases = promoted_aliases_from_v1_result(&request.result_artifact)?;
    refuse_unreviewed_alias_proposals(&request, &aliases)?;
    let registry_path = request.registry.join("registry.json");
    let registry_original =
        fs::read(&registry_path).map_err(|error| io_refusal(&registry_path, error))?;
    let (registry_json, registry_value) =
        parse_registry_json(&request.registry, &registry_original)?;
    validate_v1_registry_snapshot(&registry_json, &request.result_artifact)?;
    let requested_version = validate_requested_version(&request.next_version)?;
    let next_version = validate_next_version(&registry_json.version, &requested_version)?;

    let alias_path = request.registry.join("aliases.json");
    let alias_original = if alias_path.exists() {
        fs::read(&alias_path).map_err(|error| io_refusal(&alias_path, error))?
    } else {
        b"[]\n".to_vec()
    };
    let existing_aliases = parse_alias_entries(&request.registry, &alias_path, &alias_original)?;
    let existing_inputs = existing_aliases
        .iter()
        .map(|alias| alias.input.clone())
        .collect::<BTreeSet<_>>();
    for alias in &aliases {
        if existing_inputs.contains(&alias.input) {
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
    let profile = v1_profile_reference(&request.result_artifact)?;
    let registry_bytes =
        build_registry_bytes(registry_value, &next_version, entry_count_after, &profile)?;
    let alias_bytes = build_alias_bytes(&alias_original, &aliases)?;
    let planned_mutations = vec![
        planned_file_mutation(&alias_path, &alias_original, &alias_bytes),
        planned_file_mutation(&registry_path, &registry_original, &registry_bytes),
    ];
    let _guard = acquire_registry_mutation_guard(&request.registry)
        .map_err(|error| io_refusal(&request.registry, error))?;
    match validate_planned_mutations(&planned_mutations)
        .map_err(|error| io_refusal(&request.registry, error))?
    {
        PlannedMutationState::Ready => {}
        PlannedMutationState::AlreadyApplied => {
            return build_promote_v1_artifact(
                &request.result_artifact,
                &request.audit_artifact,
                &registry_json,
                &next_version,
                entry_count_after,
                aliases,
                false,
            );
        }
        PlannedMutationState::Stale {
            path,
            expected_hash,
            actual_hash,
        } => {
            return Err(stale_registry_snapshot_refusal(
                &path,
                &expected_hash,
                &actual_hash,
            ));
        }
    }

    write_atomic(&alias_path, &alias_bytes).map_err(|error| io_refusal(&alias_path, error))?;
    if let Err(error) = write_atomic(&registry_path, &registry_bytes) {
        let _ = write_atomic(&alias_path, &alias_original);
        return Err(io_refusal(&registry_path, error));
    }

    build_promote_v1_artifact(
        &request.result_artifact,
        &request.audit_artifact,
        &registry_json,
        &next_version,
        entry_count_after,
        aliases,
        true,
    )
}

fn refuse_unreviewed_alias_proposals(
    request: &EntityPromoteV1Request,
    aliases: &[EntityPromotedAlias],
) -> Result<(), Refusal> {
    if aliases.is_empty() {
        return Ok(());
    }
    let review_export = format!(
        "canon entity review export {} --include resolved --emit csv > review.csv",
        request.result_path.display()
    );
    let review_import = format!(
        "canon entity review import review.csv --registry {} --next-version {} --audit <AUDIT.json>",
        request.registry.display(),
        request.next_version
    );
    Err(EntityRefusalKind::ArtifactContract.to_refusal(
        "Promotion source contains alias proposals without reviewed acceptance authority",
        json!({
            "stage": "promote",
            "field": "promotable_aliases",
            "reason": "reviewed_acceptance_required",
            "result_path": request.result_path.display().to_string(),
            "proposal_count": aliases.len(),
            "review_export_command": &review_export,
            "review_import_command": &review_import,
            "writes_performed": false
        }),
        Some(format!("{review_export} && {review_import}")),
    ))
}

pub fn render_promote_v1_summary(artifact: &Value) -> String {
    let registry = value_string_or(artifact, &["registry", "id"], "<registry>");
    let before = value_string_or(artifact, &["registry", "version_before"], "<before>");
    let after = value_string_or(artifact, &["registry", "version_after"], "<after>");
    let aliases = value_u64_or(artifact, &["summary", "counts", "promoted_aliases"], 0);
    format!("{registry} promote v1 {before} -> {after} aliases={aliases}")
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

fn validate_registry_target(
    registry: &RegistryJson,
    audit: &EntityAuditArtifact,
) -> Result<(), Refusal> {
    let expected = audit.metadata.registry_snapshot.id.as_str();
    if registry.id == expected {
        return Ok(());
    }

    Err(EntityRefusalKind::RegistrySnapshot.to_refusal(
        "Promotion target registry id does not match the audited registry snapshot",
        json!({
            "stage": "promote",
            "field": "registry_id",
            "expected": expected,
            "actual": &registry.id,
            "writes_performed": false
        }),
        Some("Use the registry captured by the passing audit snapshot".to_string()),
    ))
}

fn validate_existing_profile_metadata(
    registry_value: &Value,
    profile: &EntityProfileReference,
) -> Result<(), Refusal> {
    let Some(existing) = registry_value.get("entity_profile") else {
        return Ok(());
    };

    compare_registry_profile_field(existing, "id", &profile.id)?;
    compare_registry_profile_field(existing, "version", &profile.version)?;
    compare_registry_profile_field(existing, "entity_type", &profile.entity_type)?;
    compare_registry_profile_field(existing, "identity_semantics", &profile.identity_semantics)?;
    compare_registry_profile_field(existing, "canonical_type", &profile.canonical_type)?;

    let namespaces = existing.get("patch_namespaces").unwrap_or(&Value::Null);
    compare_registry_profile_field(
        namespaces,
        "patch_namespaces.aliases",
        &profile.patch_namespaces.aliases,
    )?;
    compare_registry_profile_field(
        namespaces,
        "patch_namespaces.distinct",
        &profile.patch_namespaces.distinct,
    )?;
    compare_registry_profile_field(
        namespaces,
        "patch_namespaces.relations",
        &profile.patch_namespaces.relations,
    )?;

    if let Some(expected) = &profile.content_hash {
        compare_registry_profile_field(existing, "content_hash", expected)?;
    }

    Ok(())
}

fn registry_profile_metadata_matches(
    registry_value: &Value,
    profile: &EntityProfileReference,
) -> bool {
    registry_value.get("entity_profile").is_some()
        && validate_existing_profile_metadata(registry_value, profile).is_ok()
}

fn compare_registry_profile_field(
    value: &Value,
    field: &'static str,
    expected: &str,
) -> Result<(), Refusal> {
    let leaf = field.rsplit('.').next().unwrap_or(field);
    let actual = value
        .get(leaf)
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    if actual == expected {
        Ok(())
    } else {
        Err(registry_profile_refusal(field, expected, actual))
    }
}

fn registry_profile_refusal(field: &'static str, expected: &str, actual: &str) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Promotion target registry profile metadata does not match the audited profile",
        json!({
            "stage": "promote",
            "field": field,
            "expected": expected,
            "actual": actual,
            "writes_performed": false
        }),
        Some(
            "Use a registry whose entity_profile matches the audited profile semantics".to_string(),
        ),
    )
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
            || path.extension() != Some(OsStr::new("json"))
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

fn validate_promote_v1_source(artifact: &Value) -> Result<(), Refusal> {
    let contract = validate_artifact_v1_core_contract(artifact)?;
    validate_promote_v1_self_hash(artifact, "result")?;
    if !matches!(
        contract.artifact_version,
        CANON_ENTITY_RUN_VERSION_V1 | CANON_ENTITY_SOLVE_VERSION_V1
    ) {
        return Err(promote_refusal(
            "Promotion requires a canon_entity_run.v1 or canon_entity_solve.v1 artifact",
            json!({
                "stage": "promote",
                "field": "version",
                "expected": [CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1],
                "actual": contract.artifact_version,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_promote_v1_audit(result: &Value, audit: &Value) -> Result<(), Refusal> {
    let contract = validate_artifact_v1_core_contract(audit)?;
    if contract.artifact_version != CANON_ENTITY_AUDIT_VERSION_V1 {
        return Err(audit_gate_refusal(
            "audit_version",
            CANON_ENTITY_AUDIT_VERSION_V1,
            contract.artifact_version,
        ));
    }
    validate_promote_v1_self_hash(audit, "audit")?;
    let result_hash = required_value_string(result, &["artifact_content_hash"], "result hash")?;
    let audited_hash = required_value_string(
        audit,
        &["audited_artifact", "content_hash"],
        "audited_artifact.content_hash",
    )?;
    compare_audit_gate_field("audited_artifact_hash", result_hash, audited_hash)?;
    if value_string_or(audit, &["summary", "labels", "status"], "") != "passed" {
        return Err(audit_gate_refusal(
            "audit_status",
            "passed",
            value_string_or(audit, &["summary", "labels", "status"], "<missing>"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkBoundPromotionStage {
    Run,
    Solve,
}

fn refuse_link_bound_promotion(request: &EntityPromoteV1Request) -> Result<(), Refusal> {
    let Some((work_dir, result_stage)) =
        link_work_dir_from_result_metadata(&request.result_artifact)?
    else {
        return Ok(());
    };
    let link_path = work_dir.join(LINK_ARTIFACT_PATH);
    let stable_signal = inspect_stable_sibling_link_signal(&work_dir, &link_path);
    let link_bytes =
        match read_entity_run_committed_publication_logical_bytes(&work_dir, LINK_ARTIFACT_PATH) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                return match stable_signal {
                    StableSiblingLinkSignal::Absent => Ok(()),
                    signal => Err(stable_sibling_link_signal_refusal(
                        &link_path,
                        signal,
                        "committed_link_artifact_required",
                    )),
                };
            }
            Err(refusal)
                if committed_publication_missing_logical_file(&refusal, LINK_ARTIFACT_PATH) =>
            {
                return match stable_signal {
                    StableSiblingLinkSignal::Absent => Ok(()),
                    signal => Err(stable_sibling_link_signal_refusal(
                        &link_path,
                        signal,
                        "committed_link_artifact_missing",
                    )),
                };
            }
            Err(refusal) => {
                return Err(link_bound_source_refusal(
                    &link_path,
                    "link_artifact.committed_publication",
                    refusal,
                ));
            }
        };

    let link = validated_sibling_link_artifact_from_bytes(&link_path, &link_bytes)?;
    validate_sibling_link_binds_promotion_source(request, &work_dir, result_stage, &link)?;
    Err(link_bound_promotion_refusal(request, &link_path, &link))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StableSiblingLinkSignal {
    Absent,
    RegularFile,
    NonRegularArtifact,
    IncompleteWorkdir { link_dir: PathBuf },
    ArtifactInspectError { error: String },
    WorkdirInspectError { link_dir: PathBuf, error: String },
}

fn inspect_stable_sibling_link_signal(
    work_dir: &Path,
    link_path: &Path,
) -> StableSiblingLinkSignal {
    match fs::symlink_metadata(link_path) {
        Ok(metadata) if metadata.file_type().is_file() => StableSiblingLinkSignal::RegularFile,
        Ok(_) => StableSiblingLinkSignal::NonRegularArtifact,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let link_dir = work_dir.join("link");
            match fs::symlink_metadata(&link_dir) {
                Ok(_) => StableSiblingLinkSignal::IncompleteWorkdir { link_dir },
                Err(dir_error) if dir_error.kind() == std::io::ErrorKind::NotFound => {
                    StableSiblingLinkSignal::Absent
                }
                Err(dir_error) => StableSiblingLinkSignal::WorkdirInspectError {
                    link_dir,
                    error: dir_error.to_string(),
                },
            }
        }
        Err(error) => StableSiblingLinkSignal::ArtifactInspectError {
            error: error.to_string(),
        },
    }
}

fn stable_sibling_link_signal_refusal(
    link_path: &Path,
    signal: StableSiblingLinkSignal,
    reason: &'static str,
) -> Refusal {
    match signal {
        StableSiblingLinkSignal::Absent => link_bound_validation_refusal(
            link_path,
            "link_artifact",
            "Promotion did not find a sibling link signal",
            json!({
                "reason": reason,
                "writes_performed": false
            }),
        ),
        StableSiblingLinkSignal::RegularFile => link_bound_validation_refusal(
            link_path,
            "link_artifact",
            "Promotion requires a committed sibling link artifact",
            json!({
                "reason": reason,
                "stable_link_signal": true,
                "writes_performed": false
            }),
        ),
        StableSiblingLinkSignal::NonRegularArtifact => link_bound_validation_refusal(
            link_path,
            "link_artifact",
            "Sibling link artifact path is not a regular file",
            json!({
                "reason": "malformed_sibling_link_artifact",
                "committed_reason": reason,
                "writes_performed": false
            }),
        ),
        StableSiblingLinkSignal::IncompleteWorkdir { link_dir } => link_bound_validation_refusal(
            link_path,
            "link_artifact",
            "Sibling link workdir is incomplete",
            json!({
                "reason": "incomplete_sibling_link_workdir",
                "committed_reason": reason,
                "link_dir": link_dir.display().to_string(),
                "writes_performed": false
            }),
        ),
        StableSiblingLinkSignal::ArtifactInspectError { error } => link_bound_validation_refusal(
            link_path,
            "link_artifact",
            "Failed to inspect sibling link artifact",
            json!({
                "reason": reason,
                "error": error,
                "writes_performed": false
            }),
        ),
        StableSiblingLinkSignal::WorkdirInspectError { link_dir, error } => {
            link_bound_validation_refusal(
                &link_dir,
                "link_artifact",
                "Failed to inspect sibling link workdir",
                json!({
                    "reason": reason,
                    "error": error,
                    "writes_performed": false
                }),
            )
        }
    }
}

fn link_work_dir_from_result_metadata(
    artifact: &Value,
) -> Result<Option<(PathBuf, LinkBoundPromotionStage)>, Refusal> {
    let result_stage = match artifact.get("version").and_then(Value::as_str) {
        Some(CANON_ENTITY_RUN_VERSION_V1) => LinkBoundPromotionStage::Run,
        Some(CANON_ENTITY_SOLVE_VERSION_V1) => LinkBoundPromotionStage::Solve,
        _ => return Ok(None),
    };
    let root_dir = artifact
        .get("metadata")
        .and_then(|metadata| metadata.get("workdir"))
        .and_then(|workdir| workdir.get("root_dir"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            promote_refusal(
                "Promotion result artifact is missing workdir root metadata",
                json!({
                    "stage": "promote",
                    "field": "metadata.workdir.root_dir",
                    "writes_performed": false
                }),
            )
        })?;
    let work_dir = validate_promote_workdir_root_dir(root_dir)?;
    Ok(Some((work_dir, result_stage)))
}

fn validate_promote_workdir_root_dir(root_dir: &str) -> Result<PathBuf, Refusal> {
    let path = Path::new(root_dir);
    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::RootDir | Component::Prefix(_) => {}
            Component::CurDir | Component::ParentDir => {
                return Err(promote_refusal(
                    "Promotion result workdir root must be a safe path",
                    json!({
                        "stage": "promote",
                        "field": "metadata.workdir.root_dir",
                        "root_dir": root_dir,
                        "writes_performed": false
                    }),
                ));
            }
        }
    }
    if root_dir.trim().is_empty() || !has_normal_component {
        return Err(promote_refusal(
            "Promotion result workdir root must be nonempty",
            json!({
                "stage": "promote",
                "field": "metadata.workdir.root_dir",
                "root_dir": root_dir,
                "writes_performed": false
            }),
        ));
    }
    Ok(path.to_path_buf())
}

fn validated_sibling_link_artifact_from_bytes(
    link_path: &Path,
    link_bytes: &[u8],
) -> Result<EntityLinkArtifact, Refusal> {
    let link_value = serde_json::from_slice::<Value>(link_bytes).map_err(|error| {
        link_bound_validation_refusal(
            link_path,
            "link_artifact.json",
            "Sibling link artifact is not valid JSON",
            json!({
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    validate_entity_link_artifact_raw_shape(&link_value).map_err(|refusal| {
        link_bound_source_refusal(link_path, "link_artifact.raw_shape", refusal)
    })?;
    let link = serde_json::from_value::<EntityLinkArtifact>(link_value).map_err(|error| {
        link_bound_validation_refusal(
            link_path,
            "link_artifact",
            "Sibling link artifact is malformed",
            json!({
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    validate_entity_link_artifact_contract(&link).map_err(|refusal| {
        link_bound_source_refusal(link_path, "link_artifact.contract", refusal)
    })?;
    Ok(link)
}

fn validate_sibling_link_binds_promotion_source(
    request: &EntityPromoteV1Request,
    work_dir: &Path,
    result_stage: LinkBoundPromotionStage,
    link: &EntityLinkArtifact,
) -> Result<(), Refusal> {
    let result_hash = required_value_string(
        &request.result_artifact,
        &["artifact_content_hash"],
        "artifact_content_hash",
    )?;
    match result_stage {
        LinkBoundPromotionStage::Run => {
            validate_link_shared_reference(
                "shared_run_artifact",
                &link.shared_run_artifact,
                CANON_ENTITY_RUN_VERSION_V1,
                result_hash,
            )?;
            let solve_hash = solve_hash_from_run_value(&request.result_artifact, "result")?;
            validate_link_shared_reference(
                "shared_solve_artifact",
                &link.shared_solve_artifact,
                CANON_ENTITY_SOLVE_VERSION_V1,
                &solve_hash,
            )?;
        }
        LinkBoundPromotionStage::Solve => {
            validate_link_shared_reference(
                "shared_solve_artifact",
                &link.shared_solve_artifact,
                CANON_ENTITY_SOLVE_VERSION_V1,
                result_hash,
            )?;
            if let Some(run_bytes) =
                read_optional_sibling_artifact_bytes(work_dir, RUN_ARTIFACT_PATH)?
            {
                let run_path = work_dir.join(RUN_ARTIFACT_PATH);
                let (run_hash, run_solve_hash) =
                    validated_sibling_run_hashes(&run_path, &run_bytes)?;
                validate_link_shared_reference(
                    "shared_run_artifact",
                    &link.shared_run_artifact,
                    CANON_ENTITY_RUN_VERSION_V1,
                    &run_hash,
                )?;
                validate_link_shared_reference(
                    "shared_solve_artifact",
                    &link.shared_solve_artifact,
                    CANON_ENTITY_SOLVE_VERSION_V1,
                    &run_solve_hash,
                )?;
            }
        }
    }
    Ok(())
}

fn validated_sibling_run_hashes(
    run_path: &Path,
    run_bytes: &[u8],
) -> Result<(String, String), Refusal> {
    let run_value = serde_json::from_slice::<Value>(run_bytes).map_err(|error| {
        link_bound_validation_refusal(
            run_path,
            "run_artifact",
            "Sibling run artifact is not valid JSON",
            json!({
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    validate_promote_v1_source(&run_value)?;
    let run_hash =
        required_value_string(&run_value, &["artifact_content_hash"], "run hash")?.to_string();
    let solve_hash = solve_hash_from_run_value(&run_value, "sibling_run")?;
    Ok((run_hash, solve_hash))
}

fn solve_hash_from_run_value(
    value: &Value,
    artifact_role: &'static str,
) -> Result<String, Refusal> {
    let run = serde_json::from_value::<EntityRunArtifact>(value.clone()).map_err(|error| {
        promote_refusal(
            "Promotion source run artifact is malformed",
            json!({
                "stage": "promote",
                "field": artifact_role,
                "artifact_role": artifact_role,
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let solve_stages = run
        .stage_artifacts
        .iter()
        .filter(|stage| stage.stage == "solve" && stage.version == CANON_ENTITY_SOLVE_VERSION_V1)
        .collect::<Vec<_>>();
    if solve_stages.len() != 1 {
        return Err(promote_refusal(
            "Promotion run artifact must contain exactly one solve stage reference",
            json!({
                "stage": "promote",
                "field": format!("{artifact_role}.stage_artifacts.solve"),
                "actual_count": solve_stages.len(),
                "writes_performed": false
            }),
        ));
    }
    Ok(solve_stages[0].artifact_content_hash.clone())
}

fn validate_link_shared_reference(
    field: &'static str,
    reference: &EntityArtifactReference,
    expected_version: &str,
    expected_hash: &str,
) -> Result<(), Refusal> {
    if reference.version != expected_version || reference.content_hash != expected_hash {
        return Err(promote_refusal(
            "Sibling link artifact does not bind the submitted promotion source",
            json!({
                "stage": "promote",
                "field": field,
                "expected": {
                    "version": expected_version,
                    "content_hash": expected_hash
                },
                "actual": reference,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn link_bound_promotion_refusal(
    request: &EntityPromoteV1Request,
    link_path: &Path,
    link: &EntityLinkArtifact,
) -> Refusal {
    let review_export = format!(
        "canon entity review export {} --include escrow --emit csv > review.csv",
        link_path.display()
    );
    let review_import = format!(
        "canon entity review import review.csv --registry {} --next-version {}",
        request.registry.display(),
        request.next_version
    );
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Promotion source is bound to an entity link artifact; review/import the link artifact instead of promoting run or solve directly",
        json!({
            "stage": "promote",
            "field": "link_artifact",
            "result_path": request.result_path.display().to_string(),
            "link_artifact_path": link_path.display().to_string(),
            "link_artifact_hash": &link.artifact_content_hash,
            "shared_run_artifact": &link.shared_run_artifact,
            "shared_solve_artifact": &link.shared_solve_artifact,
            "review_export_command": &review_export,
            "review_import_command": &review_import,
            "writes_performed": false
        }),
        Some(format!("{review_export} && {review_import}")),
    )
}

fn link_bound_source_refusal(link_path: &Path, field: &'static str, refusal: Refusal) -> Refusal {
    let source_code = serde_json::to_value(&refusal.code)
        .unwrap_or_else(|_| Value::String("unknown".to_string()));
    link_bound_validation_refusal(
        link_path,
        field,
        "Promotion found a sibling link artifact but could not validate it",
        json!({
            "source_code": source_code,
            "source_message": refusal.message,
            "source_detail": refusal.detail,
            "writes_performed": false
        }),
    )
}

fn link_bound_validation_refusal(
    path: &Path,
    field: &'static str,
    message: &'static str,
    mut detail: Value,
) -> Refusal {
    if let Some(object) = detail.as_object_mut() {
        object.insert("stage".to_string(), Value::String("promote".to_string()));
        object.insert("field".to_string(), Value::String(field.to_string()));
        object.insert(
            "path".to_string(),
            Value::String(path.display().to_string()),
        );
        object.insert("writes_performed".to_string(), Value::Bool(false));
    }
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some(
            "Rerun canon entity link to rebuild link/link.json, then review/export/import the link artifact"
                .to_string(),
        ),
    )
}

fn validate_promote_v1_self_hash(artifact: &Value, artifact_role: &str) -> Result<(), Refusal> {
    validate_entity_v1_self_hash(artifact)
        .map(|_| ())
        .map_err(|refusal| {
            let source_code = serde_json::to_value(&refusal.code)
                .unwrap_or_else(|_| Value::String("unknown".to_string()));
            promote_refusal(
                "Promotion input artifact self-hash is invalid",
                json!({
                    "stage": "promote",
                    "field": format!("{artifact_role}.artifact_content_hash"),
                    "artifact_role": artifact_role,
                    "source_code": source_code,
                    "source_message": refusal.message,
                    "source_detail": refusal.detail,
                    "writes_performed": false
                }),
            )
        })
}

fn validate_v1_registry_snapshot(registry: &RegistryJson, result: &Value) -> Result<(), Refusal> {
    let expected_id = required_value_string(
        result,
        &["metadata", "registry_snapshot", "id"],
        "metadata.registry_snapshot.id",
    )?;
    let expected_version = required_value_string(
        result,
        &["metadata", "registry_snapshot", "version"],
        "metadata.registry_snapshot.version",
    )?;
    if registry.id != expected_id {
        return Err(EntityRefusalKind::RegistrySnapshot.to_refusal(
            "Promotion target registry id does not match the v1 result snapshot",
            json!({
                "stage": "promote",
                "field": "registry_id",
                "expected": expected_id,
                "actual": registry.id,
                "writes_performed": false
            }),
            Some("Use the registry captured by the result artifact".to_string()),
        ));
    }
    if registry.version != expected_version {
        return Err(EntityRefusalKind::RegistrySnapshot.to_refusal(
            "Promotion target registry version does not match the v1 result snapshot",
            json!({
                "stage": "promote",
                "field": "registry_version",
                "expected": expected_version,
                "actual": registry.version,
                "writes_performed": false
            }),
            Some("Rerun the entity result against the current registry snapshot".to_string()),
        ));
    }
    Ok(())
}

fn promoted_aliases_from_v1_result(result: &Value) -> Result<Vec<EntityPromotedAlias>, Refusal> {
    let version = result
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            promote_refusal(
                "Promotion result is missing an entity artifact version",
                json!({
                    "stage": "promote",
                    "field": "version",
                    "writes_performed": false
                }),
            )
        })?;
    reject_caller_authored_alias_authority(result, version)?;
    let solve = match version {
        CANON_ENTITY_SOLVE_VERSION_V1 => validated_solve_artifact_from_value(result, "result")?,
        CANON_ENTITY_RUN_VERSION_V1 => validated_solve_artifact_from_run(result)?,
        _ => {
            return Err(promote_refusal(
                "Promotion requires a canon_entity_run.v1 or canon_entity_solve.v1 artifact",
                json!({
                    "stage": "promote",
                    "field": "version",
                    "expected": [CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1],
                    "actual": version,
                    "writes_performed": false
                }),
            ));
        }
    };
    let aliases = solve
        .promotable_aliases
        .iter()
        .map(promoted_alias_from_solve_proposal)
        .collect::<Vec<_>>();
    validate_aliases(Path::new("."), Path::new("aliases.json"), &aliases)
}

fn validated_solve_artifact_from_run(result: &Value) -> Result<SolveArtifact, Refusal> {
    let run = serde_json::from_value::<EntityRunArtifact>(result.clone()).map_err(|error| {
        promote_refusal(
            "Promotion source run artifact is malformed",
            json!({
                "stage": "promote",
                "field": "result",
                "artifact_role": "result",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let solve_path = run_solve_artifact_path(result, &run)?;
    let solve_bytes = read_required_sibling_artifact_bytes(
        &solve_path,
        Path::new(run.work_dir.solve_artifact_path.as_str()),
        "work_dir.solve_artifact_path",
        "Promotion could not read the run-bound solve artifact",
    )?;
    let solve_value = serde_json::from_slice::<Value>(&solve_bytes).map_err(|error| {
        promote_refusal(
            "Promotion run-bound solve artifact is not JSON",
            json!({
                "stage": "promote",
                "field": "work_dir.solve_artifact_path",
                "path": solve_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let solve = validated_solve_artifact_from_value(&solve_value, "solve_artifact")?;
    validate_run_profile_matches_solve(result, &solve)?;
    validate_run_solve_hash_binding(&run, &solve)?;
    Ok(solve)
}

fn read_optional_sibling_artifact_bytes(
    work_dir: &Path,
    logical_path: &'static str,
) -> Result<Option<Vec<u8>>, Refusal> {
    match read_entity_run_committed_publication_logical_bytes(work_dir, logical_path) {
        Ok(Some(bytes)) => Ok(Some(bytes)),
        Ok(None) => {
            let stable_path = work_dir.join(logical_path);
            if stable_path.is_file() {
                fs::read(&stable_path)
                    .map(Some)
                    .map_err(|error| io_refusal(&stable_path, error))
            } else {
                Ok(None)
            }
        }
        Err(refusal) if committed_publication_missing_logical_file(&refusal, logical_path) => {
            let stable_path = work_dir.join(logical_path);
            if stable_path.is_file() {
                Err(link_bound_source_refusal(
                    &stable_path,
                    "sibling_artifact.committed_publication",
                    refusal,
                ))
            } else {
                Ok(None)
            }
        }
        Err(refusal) => Err(refusal),
    }
}

fn read_required_sibling_artifact_bytes(
    stable_path: &Path,
    logical_path: &Path,
    field: &'static str,
    missing_message: &'static str,
) -> Result<Vec<u8>, Refusal> {
    let work_dir = stable_path.parent().and_then(Path::parent).ok_or_else(|| {
        promote_refusal(
            "Promotion run solve artifact path must be workdir-relative",
            json!({
                "stage": "promote",
                "field": field,
                "path": stable_path.display().to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let logical = logical_path.to_str().ok_or_else(|| {
        promote_refusal(
            "Promotion run solve artifact path must be UTF-8",
            json!({
                "stage": "promote",
                "field": field,
                "path": logical_path.display().to_string(),
                "writes_performed": false
            }),
        )
    })?;
    match read_entity_run_committed_publication_logical_bytes(work_dir, logical) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => fs::read(stable_path).map_err(|error| {
            promote_refusal(
                missing_message,
                json!({
                    "stage": "promote",
                    "field": field,
                    "path": stable_path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        }),
        Err(refusal) => Err(refusal),
    }
}

fn committed_publication_missing_logical_file(refusal: &Refusal, logical_path: &str) -> bool {
    refusal
        .detail
        .get("publication_stage")
        .and_then(Value::as_str)
        == Some("entity_run_stage_set")
        && refusal.detail.get("logical_path").and_then(Value::as_str) == Some(logical_path)
        && refusal.detail.get("committed").and_then(Value::as_bool) == Some(true)
}

fn validated_solve_artifact_from_value(
    value: &Value,
    artifact_role: &'static str,
) -> Result<SolveArtifact, Refusal> {
    validate_promote_v1_self_hash(value, artifact_role)?;
    let solve = serde_json::from_value::<SolveArtifact>(value.clone()).map_err(|error| {
        promote_refusal(
            "Promotion source solve artifact is malformed",
            json!({
                "stage": "promote",
                "field": artifact_role,
                "artifact_role": artifact_role,
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    validate_solve_artifact_envelope_contract(&solve)
        .map(|_| solve)
        .map_err(|refusal| {
            promote_source_contract_refusal(artifact_role, "solve_artifact", refusal)
        })
}

fn validate_run_profile_matches_solve(
    result: &Value,
    solve: &SolveArtifact,
) -> Result<(), Refusal> {
    let run_profile = result
        .get("metadata")
        .and_then(|metadata| metadata.get("profile"))
        .ok_or_else(|| {
            promote_refusal(
                "Promotion run artifact is missing profile metadata",
                json!({
                    "stage": "promote",
                    "field": "metadata.profile",
                    "writes_performed": false
                }),
            )
        })?;
    let solve_profile = serde_json::to_value(&solve.metadata.profile).map_err(|error| {
        promote_refusal(
            "Promotion could not serialize solve profile metadata",
            json!({
                "stage": "promote",
                "field": "solve_artifact.metadata.profile",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    if run_profile != &solve_profile {
        return Err(promote_refusal(
            "Promotion run profile metadata does not match the bound solve artifact",
            json!({
                "stage": "promote",
                "field": "metadata.profile",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn run_solve_artifact_path(result: &Value, run: &EntityRunArtifact) -> Result<PathBuf, Refusal> {
    let root_dir = result
        .get("metadata")
        .and_then(|metadata| metadata.get("workdir"))
        .and_then(|workdir| workdir.get("root_dir"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            promote_refusal(
                "Promotion run artifact is missing workdir root metadata",
                json!({
                    "stage": "promote",
                    "field": "metadata.workdir.root_dir",
                    "writes_performed": false
                }),
            )
        })?;
    let relative = run.work_dir.solve_artifact_path.as_str();
    if root_dir.trim().is_empty()
        || relative.trim().is_empty()
        || Path::new(relative).is_absolute()
        || Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(promote_refusal(
            "Promotion run solve artifact path must be safe and workdir-relative",
            json!({
                "stage": "promote",
                "field": "work_dir.solve_artifact_path",
                "root_dir": root_dir,
                "path": relative,
                "writes_performed": false
            }),
        ));
    }
    Ok(Path::new(root_dir).join(relative))
}

fn validate_run_solve_hash_binding(
    run: &EntityRunArtifact,
    solve: &SolveArtifact,
) -> Result<(), Refusal> {
    let solve_hash = solve.artifact_content_hash.as_str();
    let solve_stages = run
        .stage_artifacts
        .iter()
        .filter(|stage| stage.stage == "solve")
        .collect::<Vec<_>>();
    if solve_stages.len() != 1 {
        return Err(promote_refusal(
            "Promotion run artifact must contain exactly one solve stage reference",
            json!({
                "stage": "promote",
                "field": "stage_artifacts.solve",
                "actual_count": solve_stages.len(),
                "writes_performed": false
            }),
        ));
    }
    let solve_stage = solve_stages[0];
    if solve_stage.version != CANON_ENTITY_SOLVE_VERSION_V1 {
        return Err(promote_refusal(
            "Promotion run solve stage reference has the wrong version",
            json!({
                "stage": "promote",
                "field": "stage_artifacts.solve.version",
                "expected": CANON_ENTITY_SOLVE_VERSION_V1,
                "actual": solve_stage.version,
                "writes_performed": false
            }),
        ));
    }
    if solve_stage.path != run.work_dir.solve_artifact_path {
        return Err(promote_refusal(
            "Promotion run solve stage path does not match work_dir.solve_artifact_path",
            json!({
                "stage": "promote",
                "field": "stage_artifacts.solve.path",
                "expected": run.work_dir.solve_artifact_path,
                "actual": solve_stage.path,
                "writes_performed": false
            }),
        ));
    }
    if solve_stage.artifact_content_hash != solve_hash {
        return Err(promote_refusal(
            "Promotion run solve stage reference does not match the bound solve artifact",
            json!({
                "stage": "promote",
                "field": "stage_artifacts.solve.artifact_content_hash",
                "expected": solve_hash,
                "actual": solve_stage.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    let metadata_refs = run
        .metadata
        .upstream_artifacts
        .iter()
        .filter(|reference| reference.version == CANON_ENTITY_SOLVE_VERSION_V1)
        .collect::<Vec<_>>();
    if metadata_refs.len() != 1 || metadata_refs[0].content_hash != solve_hash {
        return Err(promote_refusal(
            "Promotion run metadata does not bind the run-bound solve artifact",
            json!({
                "stage": "promote",
                "field": "metadata.upstream_artifacts",
                "expected_version": CANON_ENTITY_SOLVE_VERSION_V1,
                "expected_content_hash": solve_hash,
                "actual_count": metadata_refs.len(),
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn reject_caller_authored_alias_authority(result: &Value, version: &str) -> Result<(), Refusal> {
    let forbidden_fields: &[&str] = if version == CANON_ENTITY_SOLVE_VERSION_V1 {
        &["promotion_aliases", "aliases"]
    } else {
        &["promotable_aliases", "promotion_aliases", "aliases"]
    };
    for field in forbidden_fields {
        if result.get(*field).is_some() {
            return Err(promote_refusal(
                "Promotion aliases must derive from canonical solve alias proposals",
                json!({
                    "stage": "promote",
                    "field": field,
                    "writes_performed": false
                }),
            ));
        }
    }
    if result
        .get("entities")
        .and_then(Value::as_array)
        .is_some_and(|entities| {
            entities
                .iter()
                .any(|entity| entity.get("alias_inputs").is_some())
        })
    {
        return Err(promote_refusal(
            "Promotion refuses fallback entity alias inputs",
            json!({
                "stage": "promote",
                "field": "entities.alias_inputs",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn promoted_alias_from_solve_proposal(proposal: &SolveAliasProposal) -> EntityPromotedAlias {
    EntityPromotedAlias {
        input: proposal.input.clone(),
        canonical_id: proposal.canonical_id.clone(),
        canonical_type: proposal.canonical_type.clone(),
        rule_id: proposal.rule_id.clone(),
    }
}

fn promote_source_contract_refusal(
    artifact_role: &'static str,
    field: &'static str,
    refusal: Refusal,
) -> Refusal {
    let source_code = serde_json::to_value(&refusal.code)
        .unwrap_or_else(|_| Value::String("unknown".to_string()));
    promote_refusal(
        "Promotion source solve artifact contract is invalid",
        json!({
            "stage": "promote",
            "field": field,
            "artifact_role": artifact_role,
            "source_code": source_code,
            "source_message": refusal.message,
            "source_detail": refusal.detail,
            "writes_performed": false
        }),
    )
}

fn v1_profile_reference(result: &Value) -> Result<EntityProfileReference, Refusal> {
    let profile = result
        .get("metadata")
        .and_then(|metadata| metadata.get("profile"))
        .cloned()
        .ok_or_else(|| {
            promote_refusal(
                "Promotion result is missing profile metadata",
                json!({
                    "stage": "promote",
                    "field": "metadata.profile",
                    "writes_performed": false
                }),
            )
        })?;
    serde_json::from_value::<EntityProfileReference>(profile).map_err(|error| {
        promote_refusal(
            "Promotion result profile metadata is malformed",
            json!({
                "stage": "promote",
                "field": "metadata.profile",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

fn build_promote_v1_artifact(
    result: &Value,
    audit: &Value,
    registry: &RegistryJson,
    next_version: &str,
    entry_count_after: usize,
    aliases: Vec<EntityPromotedAlias>,
    wrote_registry: bool,
) -> Result<Value, Refusal> {
    let metadata = entity_v1_lifecycle_metadata_from_source(
        audit,
        EntityArtifactStageV1::Promote,
        vec![entity_v1_artifact_reference(audit)?],
    )?;
    let result_hash = required_value_string(result, &["artifact_content_hash"], "result hash")?;
    let audit_hash = required_value_string(audit, &["artifact_content_hash"], "audit hash")?;
    let alias_values = aliases
        .iter()
        .map(|alias| {
            json!({
                "input": alias.input,
                "canonical_id": alias.canonical_id,
                "canonical_type": alias.canonical_type,
                "rule_id": alias.rule_id
            })
        })
        .collect::<Vec<_>>();
    let mut artifact = json!({
        "version": CANON_ENTITY_PROMOTE_VERSION_V1,
        "artifact_content_hash": "",
        "metadata": metadata,
        "summary": {
            "counts": {
                "promoted_aliases": alias_values.len() as u64,
                "registry_entries_after": entry_count_after as u64
            },
            "labels": {
                "stage": "promote",
                "status": "applied"
            }
        },
        "promotion_manifest_path": "promote/sidecar.json",
        "source_result": {
            "content_hash": result_hash
        },
        "audit": {
            "version": CANON_ENTITY_AUDIT_VERSION_V1,
            "content_hash": audit_hash
        },
        "registry": {
            "id": registry.id,
            "version_before": registry.version,
            "version_after": next_version,
            "entry_count_before": registry.entry_count,
            "entry_count_after": entry_count_after,
            "wrote_registry": wrote_registry
        },
        "aliases": alias_values
    });
    finalize_entity_v1_self_hash(&mut artifact)?;
    Ok(artifact)
}

fn build_registry_bytes(
    mut registry_value: Value,
    next_version: &str,
    entry_count_after: usize,
    profile: &EntityProfileReference,
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
    object.insert(
        "entity_profile".to_string(),
        registry_profile_metadata(profile),
    );
    to_pretty_bytes(&registry_value)
}

fn registry_profile_metadata(profile: &EntityProfileReference) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("id".to_string(), Value::String(profile.id.clone()));
    object.insert(
        "version".to_string(),
        Value::String(profile.version.clone()),
    );
    object.insert(
        "entity_type".to_string(),
        Value::String(profile.entity_type.clone()),
    );
    object.insert(
        "identity_semantics".to_string(),
        Value::String(profile.identity_semantics.clone()),
    );
    object.insert(
        "canonical_type".to_string(),
        Value::String(profile.canonical_type.clone()),
    );
    object.insert(
        "patch_namespaces".to_string(),
        Value::Object({
            let mut namespaces = serde_json::Map::new();
            namespaces.insert(
                "aliases".to_string(),
                Value::String(profile.patch_namespaces.aliases.clone()),
            );
            namespaces.insert(
                "distinct".to_string(),
                Value::String(profile.patch_namespaces.distinct.clone()),
            );
            namespaces.insert(
                "relations".to_string(),
                Value::String(profile.patch_namespaces.relations.clone()),
            );
            namespaces
        }),
    );
    if let Some(content_hash) = &profile.content_hash {
        object.insert(
            "content_hash".to_string(),
            Value::String(content_hash.clone()),
        );
    }
    Value::Object(object)
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
        "{file_name}.canon-promote.{}.{}.tmp",
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
    let prefix = format!("{file_name}.canon-promote.");
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

fn stale_registry_snapshot_refusal(path: &Path, expected_hash: &str, actual_hash: &str) -> Refusal {
    EntityRefusalKind::RegistrySnapshot.to_refusal(
        "Current registry snapshot changed before promotion commit",
        json!({
            "stage": "promote",
            "field": "registry_snapshot_hash",
            "path": path.display().to_string(),
            "expected": expected_hash,
            "actual": actual_hash,
            "expected_registry_snapshot_hash": expected_hash,
            "actual_registry_snapshot_hash": actual_hash,
            "writes_performed": false
        }),
        Some(
            "Re-run promote from the current registry snapshot before applying aliases".to_string(),
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
