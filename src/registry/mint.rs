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
        let alias_path = add_entry::validate_alias_file(&request.registry, alias_file)?;
        let alias_bytes = add_entry::build_alias_bytes_with_entries(
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
        let alias_path = add_entry::validate_alias_file(registry, alias_file)?;
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
fn commit_planned_writes(
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
    let _guard = acquire_registry_mutation_guard(registry_dir)
        .map_err(|error| add_entry::io_refusal(registry_dir, error))?;
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

    let registry_path = request.registry.join("registry.json");
    let (registry_json, registry_meta, mapping_files) = load_registry_definition(&request.registry)
        .map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;

    let allocation = next_id(RegistryNextIdRequest {
        registry: request.registry.clone(),
        prefix: request.prefix.clone(),
        zero_pad: None,
    })?;
    let namespace = format!("{}-", allocation.scheme.prefix);
    let first_number = allocation
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
    let scheme = RegistryMintScheme {
        prefix: allocation.scheme.prefix.clone(),
        zero_pad: allocation.scheme.zero_pad,
        source: if request.prefix.is_some() {
            "prefix"
        } else {
            "registry_default"
        }
        .to_string(),
    };

    let mut seen_inputs = BTreeSet::<String>::new();
    let mut minted = Vec::new();
    let mut all_aliases = Vec::new();
    for (index, item) in request.items.iter().enumerate() {
        let number = first_number.checked_add(index as u128).ok_or_else(|| {
            add_entry::bad_registry_refusal(
                &request.registry,
                "Batch would exhaust the registry ID namespace",
                json!({ "prefix": scheme.prefix }),
                "Choose a different prefix",
            )
        })?;
        let canonical_id = format!(
            "{}-{:0width$}",
            scheme.prefix,
            number,
            width = scheme.zero_pad
        );
        let canonical_type = add_entry::resolve_canonical_type(
            &request.registry,
            &canonical_id,
            Some(
                item.canonical_type
                    .as_deref()
                    .unwrap_or(request.canonical_type.as_str()),
            ),
            &mapping_files,
        )?;
        let parsed = parse_aliases(
            &request.registry,
            &canonical_id,
            &canonical_type,
            &item.with_alias,
            None,
            &mapping_files,
        )?;
        for alias in &parsed {
            if !seen_inputs.insert(alias.input.clone()) {
                return Err(add_entry::parse_refusal(
                    &request.registry,
                    "Duplicate input inside mint-batch request",
                    json!({ "input": alias.input, "item_index": index }),
                    "Remove the duplicate alias input, then rerun; nothing was written",
                ));
            }
        }
        minted.push(RegistryMintBatchEntry {
            canonical_id,
            canonical_type,
            aliases: parsed
                .iter()
                .map(RegistryAliasWriteEntry::output_entry)
                .collect(),
        });
        all_aliases.extend(parsed);
    }

    let version_after = add_entry::resolve_next_version(
        &request.registry,
        &registry_json.version,
        request.bump,
        request.next_version.as_deref(),
    )?;
    let entry_count_after = registry_json
        .entry_count
        .checked_add(all_aliases.len())
        .ok_or_else(|| {
            add_entry::bad_registry_refusal(
                &request.registry,
                "Registry entry_count is too large to increment",
                json!({
                    "entry_count": registry_json.entry_count,
                    "aliases_to_add": all_aliases.len(),
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
    for alias in all_aliases {
        aliases_by_file
            .entry(alias.alias_file.clone())
            .or_default()
            .push(alias);
    }
    let mut alias_writes = Vec::new();
    for (alias_file, file_aliases) in &aliases_by_file {
        let alias_path = add_entry::validate_alias_file(&request.registry, alias_file)?;
        let alias_bytes = add_entry::build_alias_bytes_with_entries(
            &request.registry,
            &alias_path,
            file_aliases,
        )?;
        alias_writes.push((alias_path, alias_bytes));
    }

    let mut touched_files = aliases_by_file.keys().cloned().collect::<Vec<_>>();
    touched_files.push("registry.json".to_string());
    let mut output = RegistryMintBatchOutput {
        version: "canon_registry_mint_batch.v0".to_string(),
        registry: RegistryAddEntryRegistry {
            id: registry_meta.id,
            source: registry_meta.source,
            version_before: registry_json.version,
            version_after,
            entry_count_before: registry_json.entry_count,
            entry_count_after,
        },
        scheme,
        minted,
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
    match commit_planned_writes(
        &request.registry,
        &registry_path,
        &registry_bytes,
        &alias_writes,
        !request.no_lint,
    )? {
        CommitOutcome::Committed(Some((lint, warnings))) => {
            output.lint = lint;
            output.warnings = warnings;
        }
        CommitOutcome::Committed(None) | CommitOutcome::AlreadyApplied => {}
    }
    Ok(output)
}
