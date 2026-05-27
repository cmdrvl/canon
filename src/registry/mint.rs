use super::{
    MappingFile, add_entry, load_registry_definition,
    next_id::{RegistryNextIdRequest, next_id},
};
use crate::{
    Refusal,
    registry::add_entry::{
        RegistryAddEntryAliasEntry, RegistryAddEntryLintSummary, RegistryAddEntryRegistry,
        RegistryVersionBump,
    },
    registry_lint::{RegistryLintProfile, RegistryLintSeverity},
};
use serde::Serialize;
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
    let plan = plan_mint(request)?;
    commit_mint_plan(plan)
}

fn plan_mint(request: RegistryMintRequest) -> Result<RegistryMintPlan, Refusal> {
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

    let mut aliases_by_file = BTreeMap::<String, Vec<RegistryAddEntryAliasEntry>>::new();
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
        aliases.extend(file_aliases.iter().cloned());
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
    mapping_files: &[MappingFile],
) -> Result<Vec<RegistryAddEntryAliasEntry>, Refusal> {
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
        add_entry::ensure_input_is_new(registry, &input, mapping_files)?;
        if !seen_inputs.insert(input.clone()) {
            return Err(add_entry::parse_refusal(
                registry,
                "Duplicate input inside mint request",
                json!({ "input": input }),
                "Remove duplicate --with-alias inputs, then rerun",
            ));
        }
        aliases.push(RegistryAddEntryAliasEntry {
            alias_file,
            input,
            canonical_id: canonical_id.to_string(),
            canonical_type: canonical_type.to_string(),
            rule_id,
        });
    }
    Ok(aliases)
}

fn commit_mint_plan(mut plan: RegistryMintPlan) -> Result<RegistryMintOutput, Refusal> {
    let mut originals = BTreeMap::<PathBuf, Vec<u8>>::new();
    originals.insert(
        plan.registry_path.clone(),
        fs::read(&plan.registry_path)
            .map_err(|error| add_entry::io_refusal(&plan.registry_path, error))?,
    );
    for (path, _) in &plan.alias_writes {
        originals.insert(
            path.clone(),
            fs::read(path).map_err(|error| add_entry::io_refusal(path, error))?,
        );
    }

    for (path, bytes) in &plan.alias_writes {
        if let Err(error) = add_entry::write_atomic(path, bytes) {
            restore_originals(&originals)?;
            return Err(add_entry::io_refusal(path, error));
        }
    }
    if let Err(error) = add_entry::write_atomic(&plan.registry_path, &plan.registry_bytes) {
        restore_originals(&originals)?;
        return Err(add_entry::io_refusal(&plan.registry_path, error));
    }

    if plan.lint_enabled {
        match crate::registry_lint::lint(&plan.registry_dir, RegistryLintProfile::Standard) {
            Ok(lint) if lint.summary.errors == 0 => {
                plan.output.lint = add_entry::lint_summary(&lint);
                plan.output.warnings = lint
                    .findings
                    .iter()
                    .filter(|finding| finding.severity != RegistryLintSeverity::Error)
                    .map(|finding| finding.code.clone())
                    .collect();
            }
            Ok(lint) => {
                restore_originals(&originals)?;
                return Err(add_entry::bad_registry_refusal(
                    &plan.registry_dir,
                    "Registry mint lint failed after proposed write",
                    json!({ "lint": lint }),
                    "Fix registry lint errors or rerun with --no-lint only after manual review",
                ));
            }
            Err(refusal) => {
                restore_originals(&originals)?;
                return Err(refusal);
            }
        }
    }

    Ok(plan.output)
}

fn restore_originals(originals: &BTreeMap<PathBuf, Vec<u8>>) -> Result<(), Refusal> {
    for (path, bytes) in originals {
        add_entry::write_atomic(path, bytes).map_err(|error| add_entry::io_refusal(path, error))?;
    }
    Ok(())
}
