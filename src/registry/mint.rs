use super::{
    MappingFile, PlannedMutationState, acquire_registry_mutation_guard, add_entry,
    load_registry_definition,
    next_id::{RegistryNextIdRequest, next_id},
    planned_file_mutation, validate_planned_mutations,
};
use crate::{
    Refusal,
    identity_scope::IdentityScope,
    registry::add_entry::{
        RegistryAddEntryAliasEntry, RegistryAddEntryLintSummary, RegistryAddEntryRegistry,
        RegistryAliasWriteEntry, RegistryVersionBump,
    },
    registry_lint::{RegistryLintProfile, RegistryLintSeverity},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct RegistryMintRequest {
    pub registry: PathBuf,
    pub canonical_id: Option<String>,
    pub prefix: Option<String>,
    pub canonical_type: String,
    pub with_alias: Vec<String>,
    pub bump: Option<RegistryVersionBump>,
    pub next_version: Option<String>,
    pub no_lint: bool,
    /// Create any `--with-alias` root alias file that does not exist yet
    /// (starting from `[]`) inside the same transactional write.
    pub create_alias_file: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryMintScheme {
    pub prefix: String,
    pub zero_pad: usize,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryMintOutput {
    pub version: String,
    pub registry: RegistryAddEntryRegistry,
    pub canonical_id: String,
    pub canonical_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<RegistryMintScheme>,
    pub aliases: Vec<RegistryAddEntryAliasEntry>,
    pub touched_files: Vec<String>,
    pub lint: RegistryAddEntryLintSummary,
    pub warnings: Vec<String>,
}

impl RegistryMintOutput {
    pub fn render_plain(&self) -> &str {
        &self.canonical_id
    }
}

#[derive(Debug, Clone)]
struct RegistryMintPlan {
    registry_dir: PathBuf,
    registry_path: PathBuf,
    registry_bytes: Vec<u8>,
    alias_writes: Vec<(PathBuf, Vec<u8>)>,
    lint_enabled: bool,
    create_alias_file: bool,
    output: RegistryMintOutput,
}

pub fn mint(request: RegistryMintRequest) -> Result<RegistryMintOutput, Refusal> {
    mint_with_scope(request, None)
}

pub fn mint_with_scope(
    request: RegistryMintRequest,
    scope: Option<IdentityScope>,
) -> Result<RegistryMintOutput, Refusal> {
    let plan = plan_mint_with_scope(request, scope)?;
    commit_mint_plan(plan)
}

fn plan_mint_with_scope(
    request: RegistryMintRequest,
    scope: Option<IdentityScope>,
) -> Result<RegistryMintPlan, Refusal> {
    if request.canonical_id.is_some() && request.prefix.is_some() {
        return Err(add_entry::parse_refusal(
            &request.registry,
            "--canonical-id and --prefix are mutually exclusive",
            json!({}),
            "Use either --canonical-id <ID> or --prefix <PREFIX>, not both",
        ));
    }
    if request.with_alias.is_empty() {
        return Err(add_entry::parse_refusal(
            &request.registry,
            "canon registry mint requires at least one --with-alias",
            json!({}),
            "canon registry mint --with-alias aliases.json='Input:RULE' ...",
        ));
    }
    let scope = add_entry::finalize_requested_scope(
        &request.registry,
        scope,
        "canon registry mint --scope DIMENSION=VALUE ...",
    )?;

    let registry_path = request.registry.join("registry.json");
    let (registry_json, registry_meta, mapping_files) = load_registry_definition(&request.registry)
        .map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;

    let (canonical_id, scheme) = match request.canonical_id.as_deref() {
        Some(canonical_id) => {
            let canonical_id = add_entry::validate_trimmed_non_empty(
                &request.registry,
                "--canonical-id",
                canonical_id,
                false,
                "canon registry mint --canonical-id <ID> ...",
            )?;
            add_entry::validate_default_id_scheme(
                &request.registry,
                &canonical_id,
                registry_json.default_id_scheme.as_ref(),
            )?;
            (canonical_id, None)
        }
        None => {
            let allocation = next_id(RegistryNextIdRequest {
                registry: request.registry.clone(),
                prefix: request.prefix.clone(),
                zero_pad: None,
            })?;
            let source = if request.prefix.is_some() {
                "prefix"
            } else {
                "registry_default"
            };
            (
                allocation.next_id,
                Some(RegistryMintScheme {
                    prefix: allocation.scheme.prefix,
                    zero_pad: allocation.scheme.zero_pad,
                    source: source.to_string(),
                }),
            )
        }
    };

    let canonical_type = add_entry::resolve_canonical_type(
        &request.registry,
        &canonical_id,
        Some(request.canonical_type.as_str()),
        &mapping_files,
    )?;
    let parsed_aliases = parse_aliases(
        &request.registry,
        &canonical_id,
        &canonical_type,
        &request.with_alias,
        scope.as_ref(),
        &mapping_files,
        request.create_alias_file,
    )?;
    let version_after = add_entry::resolve_next_version(
        &request.registry,
        &registry_json.version,
        request.bump,
        request.next_version.as_deref(),
    )?;
    let entry_count_after = registry_json
        .entry_count
        .checked_add(parsed_aliases.len())
        .ok_or_else(|| {
            add_entry::bad_registry_refusal(
                &request.registry,
                "Registry entry_count is too large to increment",
                json!({
                    "entry_count": registry_json.entry_count,
                    "aliases_to_add": parsed_aliases.len(),
                }),
                "Repair registry.json entry_count, then rerun",
            )
        })?;
    let registry_bytes = add_entry::build_registry_bytes(
        &request.registry,
        &registry_path,
        &version_after,
        entry_count_after,
    )?;

    let mut aliases_by_file = BTreeMap::<String, Vec<RegistryAliasWriteEntry>>::new();
    for alias in parsed_aliases {
        aliases_by_file
            .entry(alias.alias_file.clone())
            .or_default()
            .push(alias);
    }

    let mut alias_writes = Vec::new();
    let mut aliases = Vec::new();
    for (alias_file, file_aliases) in &aliases_by_file {
        let alias_path = add_entry::validate_alias_file_allowing_missing(
            &request.registry,
            alias_file,
            request.create_alias_file,
        )?;
        let alias_bytes = add_entry::build_alias_bytes_creating_if_missing(
            &request.registry,
            &alias_path,
            file_aliases,
        )?;
        aliases.extend(
            file_aliases
                .iter()
                .map(RegistryAliasWriteEntry::output_entry),
        );
        alias_writes.push((alias_path, alias_bytes));
    }

    let mut touched_files = aliases_by_file.keys().cloned().collect::<Vec<_>>();
    touched_files.push("registry.json".to_string());
    let output = RegistryMintOutput {
        version: "canon_registry_mint.v0".to_string(),
        registry: RegistryAddEntryRegistry {
            id: registry_meta.id,
            source: registry_meta.source,
            version_before: registry_json.version,
            version_after,
            entry_count_before: registry_json.entry_count,
            entry_count_after,
        },
        canonical_id,
        canonical_type,
        scheme,
        aliases,
        touched_files,
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

    Ok(RegistryMintPlan {
        registry_dir: request.registry,
        registry_path,
        registry_bytes,
        alias_writes,
        lint_enabled: !request.no_lint,
        create_alias_file: request.create_alias_file,
        output,
    })
}

fn parse_aliases(
    registry: &Path,
    canonical_id: &str,
    canonical_type: &str,
    raw_aliases: &[String],
    scope: Option<&IdentityScope>,
    mapping_files: &[MappingFile],
    allow_missing_alias_file: bool,
) -> Result<Vec<RegistryAliasWriteEntry>, Refusal> {
    let mut seen_inputs = BTreeSet::<String>::new();
    let mut aliases = Vec::new();
    for raw in raw_aliases {
        let (alias_file, rest) = raw.split_once('=').ok_or_else(|| {
            add_entry::parse_refusal(
                registry,
                "Invalid --with-alias; expected FILE=INPUT:RULE_ID",
                json!({ "with_alias": raw }),
                "Use --with-alias aliases.json='Input:RULE_ID'",
            )
        })?;
        let (input, rule_id) = rest.rsplit_once(':').ok_or_else(|| {
            add_entry::parse_refusal(
                registry,
                "Invalid --with-alias; expected FILE=INPUT:RULE_ID",
                json!({ "with_alias": raw }),
                "Use --with-alias aliases.json='Input:RULE_ID'",
            )
        })?;
        let alias_path = add_entry::validate_alias_file_allowing_missing(
            registry,
            alias_file,
            allow_missing_alias_file,
        )?;
        let alias_file = alias_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(alias_file)
            .to_string();
        let input = add_entry::validate_trimmed_non_empty(
            registry,
            "--with-alias input",
            input,
            true,
            "Use --with-alias aliases.json='Trimmed input:RULE_ID'",
        )?;
        let rule_id = add_entry::validate_trimmed_non_empty(
            registry,
            "--with-alias rule_id",
            rule_id,
            false,
            "Use --with-alias aliases.json='Input:RULE_ID'",
        )?;
        add_entry::ensure_input_is_new_in_scope(registry, &input, scope, mapping_files)?;
        if !seen_inputs.insert(input.clone()) {
            return Err(add_entry::parse_refusal(
                registry,
                "Duplicate input inside mint request",
                json!({ "input": input }),
                "Remove duplicate --with-alias inputs, then rerun",
            ));
        }
        aliases.push(RegistryAliasWriteEntry {
            alias_file,
            input,
            canonical_id: canonical_id.to_string(),
            canonical_type: canonical_type.to_string(),
            rule_id,
            scope: scope.cloned(),
            session_authority: None,
        });
    }
    Ok(aliases)
}

fn commit_mint_plan(mut plan: RegistryMintPlan) -> Result<RegistryMintOutput, Refusal> {
    match commit_planned_writes(
        &plan.registry_dir,
        &plan.registry_path,
        &plan.registry_bytes,
        &plan.alias_writes,
        plan.lint_enabled,
        plan.create_alias_file,
    )? {
        CommitOutcome::AlreadyApplied => {}
        CommitOutcome::Committed(Some((lint, warnings))) => {
            plan.output.lint = lint;
            plan.output.warnings = warnings;
        }
        CommitOutcome::Committed(None) => {}
    }
    Ok(plan.output)
}

enum CommitOutcome {
    AlreadyApplied,
    Committed(Option<(RegistryAddEntryLintSummary, Vec<String>)>),
}

/// Shared by `mint` and `mint-batch`: one atomic, rollback-on-failure write of
/// the alias files plus `registry.json`, then the optional lint gate.
///
/// With `create_missing`, alias files that do not exist yet are first created
/// as `[]` under the mutation guard; any failure afterwards restores the
/// originals and removes exactly the files this call created, so a refused or
/// failed mint leaves no partial registry behind.
fn commit_planned_writes(
    registry_dir: &Path,
    registry_path: &Path,
    registry_bytes: &[u8],
    alias_writes: &[(PathBuf, Vec<u8>)],
    lint_enabled: bool,
    create_missing: bool,
) -> Result<CommitOutcome, Refusal> {
    let _guard = acquire_registry_mutation_guard(registry_dir)
        .map_err(|error| add_entry::io_refusal(registry_dir, error))?;
    let mut created = Vec::<PathBuf>::new();
    if create_missing {
        for (path, _) in alias_writes {
            let missing = matches!(
                fs::symlink_metadata(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            );
            if !missing {
                continue;
            }
            if let Err(error) = add_entry::write_atomic(path, b"[]") {
                remove_created(&created);
                return Err(add_entry::io_refusal(path, error));
            }
            created.push(path.clone());
        }
    }
    let result = commit_planned_writes_locked(
        registry_dir,
        registry_path,
        registry_bytes,
        alias_writes,
        lint_enabled,
    );
    if result.is_err() {
        remove_created(&created);
    }
    result
}

fn remove_created(created: &[PathBuf]) {
    for path in created {
        let _ = fs::remove_file(path);
    }
}

fn commit_planned_writes_locked(
    registry_dir: &Path,
    registry_path: &Path,
    registry_bytes: &[u8],
    alias_writes: &[(PathBuf, Vec<u8>)],
    lint_enabled: bool,
) -> Result<CommitOutcome, Refusal> {
    let mut originals = BTreeMap::<PathBuf, Vec<u8>>::new();
    originals.insert(
        registry_path.to_path_buf(),
        fs::read(registry_path).map_err(|error| add_entry::io_refusal(registry_path, error))?,
    );
    for (path, _) in alias_writes {
        originals.insert(
            path.clone(),
            fs::read(path).map_err(|error| add_entry::io_refusal(path, error))?,
        );
    }
    let mut planned_mutations = vec![planned_file_mutation(
        registry_path,
        originals
            .get(registry_path)
            .expect("registry original bytes"),
        registry_bytes,
    )];
    for (path, bytes) in alias_writes {
        planned_mutations.push(planned_file_mutation(
            path,
            originals.get(path).expect("alias original bytes"),
            bytes,
        ));
    }
    match validate_planned_mutations(&planned_mutations)
        .map_err(|error| add_entry::io_refusal(registry_dir, error))?
    {
        PlannedMutationState::Ready => {}
        PlannedMutationState::AlreadyApplied => return Ok(CommitOutcome::AlreadyApplied),
        PlannedMutationState::Stale {
            path,
            expected_hash,
            actual_hash,
        } => {
            return Err(add_entry::bad_registry_refusal(
                registry_dir,
                "Registry mint plan is stale relative to the current on-disk snapshot",
                json!({
                    "field": "write_plan_hash",
                    "path": path.display().to_string(),
                    "expected_hash": expected_hash,
                    "actual_hash": actual_hash,
                    "writes_performed": false
                }),
                "Rebuild the mint plan against the current files, then rerun",
            ));
        }
    }

    for (path, bytes) in alias_writes {
        if let Err(error) = add_entry::write_atomic(path, bytes) {
            restore_originals(&originals)?;
            return Err(add_entry::io_refusal(path, error));
        }
    }
    if let Err(error) = add_entry::write_atomic(registry_path, registry_bytes) {
        restore_originals(&originals)?;
        return Err(add_entry::io_refusal(registry_path, error));
    }

    if !lint_enabled {
        return Ok(CommitOutcome::Committed(None));
    }
    match crate::registry_lint::lint(registry_dir, RegistryLintProfile::Standard) {
        Ok(lint) if lint.summary.errors == 0 => {
            let warnings = lint
                .findings
                .iter()
                .filter(|finding| finding.severity != RegistryLintSeverity::Error)
                .map(|finding| finding.code.clone())
                .collect();
            Ok(CommitOutcome::Committed(Some((
                add_entry::lint_summary(&lint),
                warnings,
            ))))
        }
        Ok(lint) => {
            restore_originals(&originals)?;
            Err(add_entry::bad_registry_refusal(
                registry_dir,
                "Registry mint lint failed after proposed write",
                json!({ "lint": lint }),
                "Fix registry lint errors or rerun with --no-lint only after manual review",
            ))
        }
        Err(refusal) => {
            restore_originals(&originals)?;
            Err(refusal)
        }
    }
}

fn restore_originals(originals: &BTreeMap<PathBuf, Vec<u8>>) -> Result<(), Refusal> {
    for (path, bytes) in originals {
        add_entry::write_atomic(path, bytes).map_err(|error| add_entry::io_refusal(path, error))?;
    }
    Ok(())
}

// ---- mint-batch (bd-vula) ---------------------------------------------------

/// One entity of a batch: a fresh canonical ID with its starting aliases.
/// `with_alias` uses the same `FILE=INPUT:RULE_ID` spec as `registry mint`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryMintBatchItem {
    pub with_alias: Vec<String>,
    #[serde(default)]
    pub canonical_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RegistryMintBatchRequest {
    pub registry: PathBuf,
    pub prefix: Option<String>,
    /// Default canonical type for items that do not name their own.
    pub canonical_type: String,
    pub items: Vec<RegistryMintBatchItem>,
    pub bump: Option<RegistryVersionBump>,
    pub next_version: Option<String>,
    pub no_lint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryMintBatchEntry {
    pub canonical_id: String,
    pub canonical_type: String,
    pub aliases: Vec<RegistryAddEntryAliasEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryMintBatchOutput {
    pub version: String,
    pub registry: RegistryAddEntryRegistry,
    pub scheme: RegistryMintScheme,
    pub minted: Vec<RegistryMintBatchEntry>,
    pub touched_files: Vec<String>,
    pub lint: RegistryAddEntryLintSummary,
    pub warnings: Vec<String>,
}

impl RegistryMintBatchOutput {
    pub fn render_plain(&self) -> String {
        self.minted
            .iter()
            .map(|entry| entry.canonical_id.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegistryTypedAlias {
    pub alias_file: String,
    pub input: String,
    pub canonical_id: Option<String>,
    pub new_id_slot: Option<String>,
    pub canonical_type: String,
    pub rule_id: String,
    pub skip_if_existing: bool,
    pub session_authority: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct RegistryTypedWriteRequest {
    pub registry: PathBuf,
    pub prefix: Option<String>,
    pub entries: Vec<RegistryTypedAlias>,
    pub bump: Option<RegistryVersionBump>,
    pub next_version: Option<String>,
    pub no_lint: bool,
    pub expected_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RegistryTypedMinted {
    pub group_key: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub aliases: Vec<RegistryAddEntryAliasEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RegistryTypedSkippedAlias {
    pub group_key: String,
    pub input: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RegistryTypedWriteOutput {
    pub registry: RegistryAddEntryRegistry,
    pub scheme: Option<RegistryMintScheme>,
    pub minted: Vec<RegistryTypedMinted>,
    pub skipped: Vec<RegistryTypedSkippedAlias>,
    pub touched_files: Vec<String>,
    pub lint: RegistryAddEntryLintSummary,
    pub warnings: Vec<String>,
}

pub(crate) fn commit_typed_entries(
    request: RegistryTypedWriteRequest,
) -> Result<RegistryTypedWriteOutput, Refusal> {
    if request.entries.is_empty() {
        return Err(add_entry::parse_refusal(
            &request.registry,
            "Typed registry commit requires at least one alias entry",
            json!({}),
            "Provide at least one accepted session cluster",
        ));
    }

    let registry_path = request.registry.join("registry.json");
    let (registry_json, registry_meta, mapping_files) = load_registry_definition(&request.registry)
        .map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;
    if let Some(expected_version) = request.expected_version.as_deref()
        && expected_version != registry_json.version
    {
        return Err(add_entry::bad_registry_refusal(
            &request.registry,
            "Registry changed after the typed commit plan was created",
            json!({
                "reason": "stale_registry",
                "expected_version": expected_version,
                "actual_version": registry_json.version,
                "writes_performed": false,
            }),
            "Rerun the session solve, then rerun session commit",
        ));
    }

    let mut groups = BTreeMap::<String, Vec<RegistryTypedAlias>>::new();
    let mut group_order = Vec::new();
    for entry in request.entries {
        let group_key = match (&entry.canonical_id, &entry.new_id_slot) {
            (Some(canonical_id), None) => format!("existing:{canonical_id}"),
            (None, Some(slot)) => format!("new:{slot}"),
            _ => {
                return Err(add_entry::parse_refusal(
                    &request.registry,
                    "Typed registry alias must name either an existing canonical ID or a new-ID slot",
                    json!({
                        "canonical_id": entry.canonical_id,
                        "new_id_slot": entry.new_id_slot,
                    }),
                    "Provide exactly one of canonical_id or new_id_slot",
                ));
            }
        };
        if !groups.contains_key(&group_key) {
            group_order.push(group_key.clone());
        }
        groups.entry(group_key).or_default().push(entry);
    }

    let mut first_number = None;
    let mut scheme = None;
    if group_order
        .iter()
        .any(|group_key| group_key.starts_with("new:"))
    {
        let allocation = next_id(RegistryNextIdRequest {
            registry: request.registry.clone(),
            prefix: request.prefix.clone(),
            zero_pad: None,
        })?;
        let namespace = format!("{}-", allocation.scheme.prefix);
        let number = allocation
            .next_id
            .strip_prefix(&namespace)
            .and_then(|digits| digits.parse::<u128>().ok())
            .ok_or_else(|| {
                add_entry::bad_registry_refusal(
                    &request.registry,
                    "Allocated ID does not follow the PREFIX-<digits> scheme",
                    json!({ "next_id": allocation.next_id }),
                    "Repair the registry ID namespace, then rerun",
                )
            })?;
        first_number = Some(number);
        scheme = Some(RegistryMintScheme {
            prefix: allocation.scheme.prefix,
            zero_pad: allocation.scheme.zero_pad,
            source: if request.prefix.is_some() {
                "prefix"
            } else {
                "registry_default"
            }
            .to_string(),
        });
    }

    let mut seen_inputs = BTreeSet::<String>::new();
    let mut parsed_by_group = BTreeMap::<String, Vec<RegistryAliasWriteEntry>>::new();
    let mut skipped = Vec::new();
    let mut minted = Vec::new();
    for (group_index, group_key) in group_order.iter().enumerate() {
        let entries = groups.get(group_key).ok_or_else(|| {
            add_entry::bad_registry_refusal(
                &request.registry,
                "Typed registry group disappeared during planning",
                json!({"group_key": group_key}),
                "Rerun the typed registry commit against the current registry",
            )
        })?;
        let first = entries.first().ok_or_else(|| {
            add_entry::bad_registry_refusal(
                &request.registry,
                "Typed registry group is empty",
                json!({"group_key": group_key}),
                "Provide an alias for every typed registry group",
            )
        })?;
        let canonical_id = if let Some(canonical_id) = first.canonical_id.as_deref() {
            add_entry::validate_trimmed_non_empty(
                &request.registry,
                "canonical_id",
                canonical_id,
                false,
                "Provide a non-empty canonical_id",
            )?
        } else {
            let number = first_number
                .ok_or_else(|| {
                    add_entry::bad_registry_refusal(
                        &request.registry,
                        "New typed registry group has no allocated ID",
                        json!({"group_key": group_key}),
                        "Repair the registry ID scheme, then rerun the typed commit",
                    )
                })?
                .checked_add(
                    group_order[..group_index]
                        .iter()
                        .filter(|key| key.starts_with("new:"))
                        .count() as u128,
                )
                .ok_or_else(|| {
                    add_entry::bad_registry_refusal(
                        &request.registry,
                        "Typed commit would exhaust the registry ID namespace",
                        json!({ "prefix": scheme.as_ref().map(|value| &value.prefix) }),
                        "Choose a different registry prefix",
                    )
                })?;
            let scheme_ref = scheme.as_ref().ok_or_else(|| {
                add_entry::bad_registry_refusal(
                    &request.registry,
                    "New typed registry group has no ID scheme",
                    json!({"group_key": group_key}),
                    "Repair the registry ID scheme, then rerun the typed commit",
                )
            })?;
            format!(
                "{}-{:0width$}",
                scheme_ref.prefix,
                number,
                width = scheme_ref.zero_pad
            )
        };
        if first.canonical_id.is_some()
            && !mapping_files.iter().any(|file| {
                file.entries
                    .iter()
                    .any(|entry| entry.canonical_id == canonical_id)
            })
        {
            return Err(add_entry::parse_refusal(
                &request.registry,
                "Typed registry entry names an unknown existing canonical_id",
                json!({ "canonical_id": canonical_id, "group_key": group_key }),
                "Use a canonical_id already present in the registry, then rerun",
            ));
        }
        add_entry::validate_default_id_scheme(
            &request.registry,
            &canonical_id,
            registry_json.default_id_scheme.as_ref(),
        )?;
        let canonical_type = add_entry::resolve_canonical_type(
            &request.registry,
            &canonical_id,
            Some(first.canonical_type.as_str()),
            &mapping_files,
        )?;
        let mut aliases = Vec::new();
        for entry in entries {
            if entry.canonical_type != canonical_type {
                return Err(add_entry::parse_refusal(
                    &request.registry,
                    "Typed registry entries in one group must share canonical_type",
                    json!({
                        "group_key": group_key,
                        "expected": canonical_type,
                        "actual": entry.canonical_type,
                    }),
                    "Use one entity kind per accepted cluster",
                ));
            }
            let alias_file = add_entry::validate_alias_file(&request.registry, &entry.alias_file)?;
            let alias_file = alias_file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(entry.alias_file.as_str())
                .to_string();
            let input = add_entry::validate_trimmed_non_empty(
                &request.registry,
                "typed alias input",
                &entry.input,
                false,
                "Use an ASCII-trimmed alias input",
            )?;
            let rule_id = add_entry::validate_trimmed_non_empty(
                &request.registry,
                "typed alias rule_id",
                &entry.rule_id,
                false,
                "Use a non-empty alias rule_id",
            )?;
            if let Some(existing) = mapping_files
                .iter()
                .flat_map(|file| file.entries.iter())
                .find(|existing| existing.input == input)
            {
                if !entry.skip_if_existing {
                    add_entry::ensure_input_is_new_in_scope(
                        &request.registry,
                        &input,
                        None,
                        &mapping_files,
                    )?;
                }
                skipped.push(RegistryTypedSkippedAlias {
                    group_key: group_key.clone(),
                    input,
                    reason: if existing.canonical_id == canonical_id {
                        "already_present".to_string()
                    } else {
                        "key_alias_collision".to_string()
                    },
                });
                continue;
            }
            if !seen_inputs.insert(input.clone()) {
                return Err(add_entry::parse_refusal(
                    &request.registry,
                    "Duplicate input inside typed registry commit",
                    json!({ "input": input, "group_key": group_key }),
                    "Remove duplicate aliases, then rerun",
                ));
            }
            aliases.push(RegistryAliasWriteEntry {
                alias_file,
                input,
                canonical_id: canonical_id.clone(),
                canonical_type: canonical_type.clone(),
                rule_id,
                scope: None,
                session_authority: entry.session_authority.clone(),
            });
        }
        parsed_by_group.insert(group_key.clone(), aliases);
        minted.push(RegistryTypedMinted {
            group_key: group_key.clone(),
            canonical_id,
            canonical_type,
            aliases: Vec::new(),
        });
    }

    let mut all_aliases = Vec::new();
    let mut aliases_by_file = BTreeMap::<String, Vec<RegistryAliasWriteEntry>>::new();
    for aliases in parsed_by_group.values() {
        for alias in aliases {
            aliases_by_file
                .entry(alias.alias_file.clone())
                .or_default()
                .push(alias.clone());
            all_aliases.push(alias.clone());
        }
    }
    let entry_count_before: usize = mapping_files.iter().map(|file| file.entries.len()).sum();
    let version_after = if all_aliases.is_empty() {
        registry_json.version.clone()
    } else {
        add_entry::resolve_next_version(
            &request.registry,
            &registry_json.version,
            request.bump,
            request.next_version.as_deref(),
        )?
    };
    let entry_count_after = entry_count_before
        .checked_add(all_aliases.len())
        .ok_or_else(|| {
            add_entry::bad_registry_refusal(
                &request.registry,
                "Registry entry_count is too large to recount",
                json!({ "entry_count": entry_count_before, "aliases_to_add": all_aliases.len() }),
                "Repair the registry mapping files, then rerun",
            )
        })?;
    let mut alias_writes = Vec::new();
    for (alias_file, file_aliases) in &aliases_by_file {
        let alias_path = add_entry::validate_alias_file(&request.registry, alias_file)?;
        let alias_bytes = add_entry::build_alias_bytes_with_entries(
            &request.registry,
            &alias_path,
            file_aliases,
        )?;
        alias_writes.push((alias_path, alias_bytes));
        for minted_entry in &mut minted {
            if let Some(group_aliases) = parsed_by_group.get(&minted_entry.group_key) {
                minted_entry.aliases.extend(
                    group_aliases
                        .iter()
                        .filter(|alias| alias.alias_file == *alias_file)
                        .map(RegistryAliasWriteEntry::output_entry),
                );
            }
        }
    }
    let mut touched_files = aliases_by_file.keys().cloned().collect::<Vec<_>>();
    if !all_aliases.is_empty() {
        touched_files.push("registry.json".to_string());
    }
    let mut output = RegistryTypedWriteOutput {
        registry: RegistryAddEntryRegistry {
            id: registry_meta.id,
            source: registry_meta.source,
            version_before: registry_json.version,
            version_after,
            entry_count_before,
            entry_count_after,
        },
        scheme,
        minted,
        skipped,
        touched_files,
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
    if all_aliases.is_empty() {
        return Ok(output);
    }
    let registry_bytes = add_entry::build_registry_bytes(
        &request.registry,
        &registry_path,
        &output.registry.version_after,
        output.registry.entry_count_after,
    )?;
    match commit_planned_writes(
        &request.registry,
        &registry_path,
        &registry_bytes,
        &alias_writes,
        !request.no_lint,
        false,
    )? {
        CommitOutcome::Committed(Some((lint, warnings))) => {
            output.lint = lint;
            output.warnings = warnings;
        }
        CommitOutcome::Committed(None) | CommitOutcome::AlreadyApplied => {}
    }
    Ok(output)
}

/// Read a batch file: one JSON object per line (blank lines ignored).
pub fn read_batch_items(
    registry: &Path,
    path: &Path,
) -> Result<Vec<RegistryMintBatchItem>, Refusal> {
    let text = fs::read_to_string(path).map_err(|error| add_entry::io_refusal(path, error))?;
    let mut items = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let item = serde_json::from_str::<RegistryMintBatchItem>(line).map_err(|error| {
            add_entry::parse_refusal(
                registry,
                "Invalid mint-batch line; expected {\"with_alias\": [\"FILE=INPUT:RULE_ID\", ...], \"canonical_type\": optional}",
                json!({ "line": index + 1, "error": error.to_string() }),
                "Fix the batch file line, then rerun; nothing was written",
            )
        })?;
        items.push(item);
    }
    Ok(items)
}

/// Mint N canonical IDs in ONE registry version: one atomic write, one lint
/// pass, one version bump. IDs are allocated sequentially from the registry's
/// scheme (or `--prefix`); every alias must be new, inside the batch and
/// against the registry. Any refusal leaves the registry untouched.
pub fn mint_batch(request: RegistryMintBatchRequest) -> Result<RegistryMintBatchOutput, Refusal> {
    if request.items.is_empty() {
        return Err(add_entry::parse_refusal(
            &request.registry,
            "canon registry mint-batch requires at least one batch item",
            json!({}),
            "Add one JSON object per line: {\"with_alias\": [\"aliases.json=Name:RULE\"]}",
        ));
    }
    if let Some(index) = request
        .items
        .iter()
        .position(|item| item.with_alias.is_empty())
    {
        return Err(add_entry::parse_refusal(
            &request.registry,
            "Every mint-batch item requires at least one with_alias spec",
            json!({ "item_index": index }),
            "Give each batch item at least one FILE=INPUT:RULE_ID alias",
        ));
    }

    let mut typed_entries = Vec::new();
    let mut seen_inputs = BTreeSet::new();
    for (index, item) in request.items.iter().enumerate() {
        let canonical_type = item
            .canonical_type
            .clone()
            .unwrap_or_else(|| request.canonical_type.clone());
        for raw_alias in &item.with_alias {
            let (alias_file, input, rule_id) = split_alias_spec(&request.registry, raw_alias)?;
            if !seen_inputs.insert(input.clone()) {
                return Err(add_entry::parse_refusal(
                    &request.registry,
                    "Duplicate input inside mint-batch request",
                    json!({ "input": input, "item_index": index }),
                    "Remove the duplicate alias input, then rerun; nothing was written",
                ));
            }
            typed_entries.push(RegistryTypedAlias {
                alias_file,
                input,
                canonical_id: None,
                new_id_slot: Some(index.to_string()),
                canonical_type: canonical_type.clone(),
                rule_id,
                skip_if_existing: false,
                session_authority: None,
            });
        }
    }
    let registry = request.registry.clone();
    let typed = commit_typed_entries(RegistryTypedWriteRequest {
        registry,
        prefix: request.prefix,
        entries: typed_entries,
        bump: request.bump,
        next_version: request.next_version,
        no_lint: request.no_lint,
        expected_version: None,
    })?;
    let scheme = typed.scheme.ok_or_else(|| {
        add_entry::bad_registry_refusal(
            &request.registry,
            "Mint-batch did not allocate a new canonical ID",
            json!({}),
            "Provide a fresh mint-batch item, then rerun",
        )
    })?;
    Ok(RegistryMintBatchOutput {
        version: "canon_registry_mint_batch.v0".to_string(),
        registry: typed.registry,
        scheme,
        minted: typed
            .minted
            .into_iter()
            .map(|entry| RegistryMintBatchEntry {
                canonical_id: entry.canonical_id,
                canonical_type: entry.canonical_type,
                aliases: entry.aliases,
            })
            .collect(),
        touched_files: typed.touched_files,
        lint: typed.lint,
        warnings: typed.warnings,
    })
}

fn split_alias_spec(registry: &Path, raw: &str) -> Result<(String, String, String), Refusal> {
    let (alias_file, rest) = raw.split_once('=').ok_or_else(|| {
        add_entry::parse_refusal(
            registry,
            "Invalid --with-alias; expected FILE=INPUT:RULE_ID",
            json!({ "with_alias": raw }),
            "Use --with-alias aliases.json='Input:RULE_ID'",
        )
    })?;
    let (input, rule_id) = rest.rsplit_once(':').ok_or_else(|| {
        add_entry::parse_refusal(
            registry,
            "Invalid --with-alias; expected FILE=INPUT:RULE_ID",
            json!({ "with_alias": raw }),
            "Use --with-alias aliases.json='Input:RULE_ID'",
        )
    })?;
    let input = add_entry::validate_trimmed_non_empty(
        registry,
        "--with-alias input",
        input,
        true,
        "Use --with-alias aliases.json='Trimmed input:RULE_ID'",
    )?;
    let rule_id = add_entry::validate_trimmed_non_empty(
        registry,
        "--with-alias rule_id",
        rule_id,
        false,
        "Use --with-alias aliases.json='Input:RULE_ID'",
    )?;
    Ok((alias_file.to_string(), input, rule_id))
}
