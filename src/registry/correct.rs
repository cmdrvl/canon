//! `canon registry correct`: merge two canonical IDs as a reviewed registry
//! version (bd-vdcw.52). One ID survives, the other is retired and keeps
//! resolving, exactly and typed, to the survivor through a
//! `SUPERSEDED_ID_ALIAS` entry. A correction is always a new version, is never
//! generated or applied by canon on its own, and records the evidence that
//! justified it in `corrections.jsonl`.

use super::{
    RegistryChangedFile, RegistryHandoffSide, RegistryVersionBump, acquire_registry_mutation_guard,
    add_entry::{
        bad_registry_refusal, build_registry_bytes, io_refusal, resolve_next_version,
        to_pretty_bytes, write_atomic,
    },
    changed_files, registry_identity,
};
use crate::{
    Refusal, RefusalCode,
    registry_lint::{RegistryLintProfile, RegistryLintSeverity},
    witness::hash_bytes,
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const REGISTRY_CORRECT_VERSION: &str = "canon_registry_correct.v0";
pub const REGISTRY_CORRECTION_LINE_VERSION: &str = "canon.registry.correction.v0";
pub const SUPERSEDED_ID_ALIAS: &str = "SUPERSEDED_ID_ALIAS";
const CORRECTIONS_FILE: &str = "corrections.jsonl";

mod ops;
mod supersede;

#[derive(Debug, Clone)]
pub struct RegistryCorrectRequest {
    pub registry: PathBuf,
    pub from: PathBuf,
    pub next_version: Option<String>,
    pub bump: Option<RegistryVersionBump>,
    pub rules: Option<PathBuf>,
    pub expect_base: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RegistryCorrectionSummary {
    pub correction_id: String,
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub survivor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retired: Option<String>,
    /// detach and split: the id the correction acts on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    /// split: the id minted for the moved entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_id: Option<String>,
    pub outcome_ref: String,
    pub closure_hash: String,
    /// merge: `passed` with `--rules`, `not_run` without.
    pub key_check: String,
    pub repointed_aliases: usize,
    pub repointed_key_aliases: usize,
    pub repointed_relations: usize,
    pub dropped_relations: usize,
    pub repointed_memory: usize,
    pub superseded_aliases: Vec<String>,
    /// detach: spellings and keys taken off the id; split: entries moved.
    pub detached: usize,
    pub moved: usize,
    /// Cannot-link facts written to registry memory.
    pub memory_written: usize,
    pub cannot_links_removed: usize,
    /// split: relations still naming the old id, for the reviewer.
    pub relations_left_on_old_id: usize,
    /// What the stored `corrections.jsonl` line carries beyond the common fields.
    #[serde(skip)]
    pub stored: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryCorrectOutput {
    pub version: String,
    /// `applied`, `already_applied` or `dry_run`.
    pub outcome: String,
    pub registry_id: String,
    pub version_before: String,
    pub version_after: String,
    pub corrections: Vec<RegistryCorrectionSummary>,
    pub base: RegistryHandoffSide,
    pub new: RegistryHandoffSide,
    pub changed_files: Vec<RegistryChangedFile>,
}

impl RegistryCorrectOutput {
    pub fn render_summary(&self) -> String {
        let mut lines = vec![format!(
            "outcome={} registry={} version={} -> {} corrections={} changed_files={}",
            self.outcome,
            self.registry_id,
            self.version_before,
            self.version_after,
            self.corrections.len(),
            self.changed_files.len()
        )];
        for correction in &self.corrections {
            let line = match correction.op.as_str() {
                "merge" => format!(
                    "merge {} -> {} correction_id={} key_check={} aliases={} key_aliases={} relations={} dropped_relations={}",
                    correction.retired.as_deref().unwrap_or(""),
                    correction.survivor.as_deref().unwrap_or(""),
                    correction.correction_id,
                    correction.key_check,
                    correction.repointed_aliases,
                    correction.repointed_key_aliases,
                    correction.repointed_relations,
                    correction.dropped_relations
                ),
                "supersede" => format!(
                    "supersede {} -> {} correction_id={} moved={}",
                    correction.canonical_id.as_deref().unwrap_or(""),
                    correction.survivor.as_deref().unwrap_or(""),
                    correction.correction_id,
                    correction.moved
                ),
                "detach" => format!(
                    "detach {} correction_id={} detached={} memory_written={}",
                    correction.canonical_id.as_deref().unwrap_or(""),
                    correction.correction_id,
                    correction.detached,
                    correction.memory_written
                ),
                "split" => format!(
                    "split {} -> {} correction_id={} moved={} relations_left_on_old_id={}",
                    correction.canonical_id.as_deref().unwrap_or(""),
                    correction.new_id.as_deref().unwrap_or(""),
                    correction.correction_id,
                    correction.moved,
                    correction.relations_left_on_old_id
                ),
                _ => format!(
                    "{} correction_id={} cannot_links_removed={}",
                    correction.op, correction.correction_id, correction.cannot_links_removed
                ),
            };
            lines.push(line);
        }
        lines.join("\n")
    }
}

/// One merge line as the reviewer wrote it.
#[derive(Debug, Clone)]
struct MergeLine {
    survivor: String,
    retired: String,
    reviewer: String,
    evidence_ids: Vec<String>,
    reason: String,
    source: Value,
}

impl MergeLine {
    /// The canonical correction as the reviewer wrote it; the versions and file
    /// hashes recorded on the stored line are outside it, so one correction has
    /// one closure wherever it is applied.
    fn canonical(&self) -> Value {
        json!({
            "op": "merge",
            "survivor": self.survivor,
            "retired": self.retired,
            "reviewer": self.reviewer,
            "evidence_ids": self.evidence_ids,
            "reason": self.reason,
            "source": self.source,
        })
    }

    fn closure_hash(&self) -> String {
        hash_bytes(&serde_json::to_vec(&self.canonical()).unwrap_or_default())
    }

    fn correction_id(&self) -> String {
        format!(
            "correction:{}",
            self.closure_hash().trim_start_matches("blake3:")
        )
    }

    fn outcome_ref(&self, registry_id: &str) -> String {
        let subject = hash_bytes(format!("merge|{}|{}", self.retired, self.survivor).as_bytes());
        format!(
            "cmdrvl://canon/outcome/{registry_id}/correction/{}",
            subject.trim_start_matches("blake3:")
        )
    }
}

fn refusal(registry: &Path, message: impl Into<String>, detail: Value, next: &str) -> Refusal {
    bad_registry_refusal(registry, message.into(), detail, next)
}

/// One correction as the reviewer wrote it.
#[derive(Debug, Clone)]
enum Correction {
    Merge(MergeLine),
    Detach(ops::DetachLine),
    Split(ops::SplitLine),
    RemoveCannotLink(ops::RemoveLine),
    Supersede(supersede::SupersedeLine),
}

impl Correction {
    fn canonical(&self) -> Value {
        match self {
            Self::Merge(line) => line.canonical(),
            Self::Detach(line) => line.canonical(),
            Self::Split(line) => line.canonical(),
            Self::RemoveCannotLink(line) => line.canonical(),
            Self::Supersede(line) => line.canonical(),
        }
    }

    fn closure_hash(&self) -> String {
        match self {
            Self::Merge(line) => line.closure_hash(),
            _ => hash_bytes(&serde_json::to_vec(&self.canonical()).unwrap_or_default()),
        }
    }

    fn correction_id(&self) -> String {
        format!(
            "correction:{}",
            self.closure_hash().trim_start_matches("blake3:")
        )
    }

    fn outcome_ref(&self, registry_id: &str) -> String {
        match self {
            Self::Merge(line) => line.outcome_ref(registry_id),
            _ => format!(
                "cmdrvl://canon/outcome/{registry_id}/correction/{}",
                self.closure_hash().trim_start_matches("blake3:")
            ),
        }
    }

    /// reviewer, evidence ids, reason and source, as written.
    fn review(&self) -> (&str, &[String], &str, &Value) {
        match self {
            Self::Merge(line) => (
                &line.reviewer,
                &line.evidence_ids,
                &line.reason,
                &line.source,
            ),
            Self::Detach(line) => line.review.parts(),
            Self::Split(line) => line.review.parts(),
            Self::RemoveCannotLink(line) => line.review.parts(),
            Self::Supersede(line) => line.review.parts(),
        }
    }

    fn op(&self) -> &'static str {
        match self {
            Self::Merge(_) => "merge",
            Self::Detach(_) => "detach",
            Self::Split(_) => "split",
            Self::RemoveCannotLink(_) => "remove_cannot_link",
            Self::Supersede(_) => "supersede",
        }
    }
}

fn parse_lines(registry: &Path, from: &Path) -> Result<Vec<Correction>, Refusal> {
    let text = fs::read_to_string(from).map_err(|error| io_refusal(from, error))?;
    let mut lines = Vec::new();
    for (index, raw) in text
        .lines()
        .enumerate()
        .filter(|(_, raw)| !raw.trim().is_empty())
    {
        let number = index + 1;
        let value: Value = serde_json::from_str(raw).map_err(|error| {
            refusal(
                registry,
                format!("corrections line {number} is not JSON: {error}"),
                json!({"line": number, "writes_performed": false}),
                "Fix the corrections file, then rerun canon registry correct",
            )
        })?;
        let field = |name: &str| value.get(name).and_then(Value::as_str).map(str::trim);
        let op = field("op").unwrap_or_default().to_string();
        if !matches!(
            op.as_str(),
            "merge" | "detach" | "split" | "remove_cannot_link" | "supersede"
        ) {
            return Err(refusal(
                registry,
                format!(
                    "corrections line {number}: op must be merge, detach, split, remove_cannot_link or supersede"
                ),
                json!({"line": number, "op": value.get("op"), "writes_performed": false}),
                "Use {\"op\":\"merge\"|\"detach\"|\"split\"|\"remove_cannot_link\"|\"supersede\", ...}",
            ));
        }
        let required = |name: &str| -> Result<String, Refusal> {
            field(name)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    refusal(
                        registry,
                        format!("corrections line {number}: '{name}' is required"),
                        json!({"line": number, "field": name, "writes_performed": false}),
                        "Complete the merge line, then rerun",
                    )
                })
        };
        let reviewer = required("reviewer")?;
        if !reviewer.starts_with("human:") {
            return Err(refusal(
                registry,
                format!(
                    "corrections line {number}: a correction needs a human reviewer, got '{reviewer}'"
                ),
                json!({"line": number, "reviewer": reviewer, "writes_performed": false}),
                "Set reviewer to human:<name>; canon never corrects on its own",
            ));
        }
        let mut evidence_ids = value
            .get("evidence_ids")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        evidence_ids.sort();
        evidence_ids.dedup();
        if evidence_ids.is_empty() {
            return Err(refusal(
                registry,
                format!("corrections line {number}: a correction needs at least one evidence id"),
                json!({"line": number, "writes_performed": false}),
                "List the evidence ids that justify the correction",
            ));
        }
        let reason = field("reason").unwrap_or_default().to_string();
        let source = value.get("source").cloned().unwrap_or(Value::Null);
        if op == "supersede" {
            let review = ops::Review {
                reviewer,
                evidence_ids,
                reason,
                source,
            };
            lines.push(supersede::parse(registry, &value, number, review)?);
            continue;
        }
        if op != "merge" {
            let review = ops::Review {
                reviewer,
                evidence_ids,
                reason,
                source,
            };
            lines.push(ops::parse_op(registry, &op, &value, number, review)?);
            continue;
        }
        let survivor = required("survivor")?;
        let retired = required("retired")?;
        if survivor == retired {
            return Err(refusal(
                registry,
                format!("corrections line {number}: survivor and retired are the same id"),
                json!({"line": number, "id": survivor, "writes_performed": false}),
                "Name two different canonical ids",
            ));
        }
        lines.push(Correction::Merge(MergeLine {
            survivor,
            retired,
            reviewer,
            evidence_ids,
            reason,
            source,
        }));
    }
    if lines.is_empty() {
        return Err(refusal(
            registry,
            "the corrections file holds no corrections",
            json!({"writes_performed": false}),
            "Add at least one merge line",
        ));
    }
    Ok(lines)
}

/// Retired ids recorded by corrections; `allocate_next` must never mint them
/// again.
/// One recorded succession: the old id keeps its own entries and is flagged
/// `superseded_by` the successor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuccessionRecord {
    pub successor: String,
    pub succession: String,
    pub effective_date: String,
}

/// What a lookup of a superseded id reports: the direct successor and the chain
/// to the active end (the last element).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Succession {
    pub superseded_by: String,
    pub chain: Vec<String>,
    pub succession: String,
    pub effective_date: String,
}

/// Successions recorded by `supersede` corrections, old id to its record.
pub fn read_successions(registry: &Path) -> BTreeMap<String, SuccessionRecord> {
    let Ok(text) = fs::read_to_string(registry.join(CORRECTIONS_FILE)) else {
        return BTreeMap::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line.get("op").and_then(Value::as_str) == Some("supersede"))
        .filter_map(|line| {
            let field = |name: &str| line.get(name)?.as_str().map(str::to_string);
            Some((
                field("superseded")?,
                SuccessionRecord {
                    successor: field("successor")?,
                    succession: field("succession")?,
                    effective_date: field("effective_date")?,
                },
            ))
        })
        .collect()
}

/// The succession of `id`, if it is superseded, with the chain to the active end.
pub fn succession_of(
    successions: &BTreeMap<String, SuccessionRecord>,
    id: &str,
) -> Option<Succession> {
    let record = successions.get(id)?;
    let mut chain = vec![record.successor.clone()];
    while let Some(next) = successions.get(chain.last()?) {
        if chain.contains(&next.successor) || chain.len() > successions.len() {
            break;
        }
        chain.push(next.successor.clone());
    }
    Some(Succession {
        superseded_by: record.successor.clone(),
        chain,
        succession: record.succession.clone(),
        effective_date: record.effective_date.clone(),
    })
}

pub(crate) fn retired_ids(registry: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(registry.join(CORRECTIONS_FILE)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| {
            line.get("retired")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn applied_correction_ids(registry: &Path) -> BTreeSet<String> {
    let Ok(text) = fs::read_to_string(registry.join(CORRECTIONS_FILE)) else {
        return BTreeSet::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| {
            line.get("correction_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

/// The registry files a correction can change, read as JSON where they are.
struct Model {
    registry_path: PathBuf,
    registry_id: String,
    version_before: String,
    mapping: BTreeMap<PathBuf, Vec<Value>>,
    relations: Option<(PathBuf, Vec<String>)>,
    cannot_link: Option<(PathBuf, Vec<String>)>,
    anchors: Vec<(PathBuf, Vec<String>)>,
    corrections_text: String,
}

fn read_lines(path: &Path) -> Option<Vec<String>> {
    fs::read_to_string(path).ok().map(|text| {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect()
    })
}

fn load_model(registry: &Path) -> Result<Model, Refusal> {
    let registry_path = registry.join("registry.json");
    let registry_value: Value = serde_json::from_slice(
        &fs::read(&registry_path).map_err(|error| io_refusal(&registry_path, error))?,
    )
    .map_err(|error| Refusal::bad_registry(&registry.display().to_string(), &error.to_string()))?;
    let text = |name: &str| {
        registry_value
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut files = fs::read_dir(registry)
        .map_err(|error| io_refusal(registry, error))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension() == Some("json".as_ref())
                && path.file_name() != Some("registry.json".as_ref())
                && path.file_name() != Some("_build.json".as_ref())
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut mapping = BTreeMap::new();
    for path in files {
        let bytes = fs::read(&path).map_err(|error| io_refusal(&path, error))?;
        if let Ok(Value::Array(rows)) = serde_json::from_slice::<Value>(&bytes) {
            mapping.insert(path, rows);
        }
    }
    let anchors = fs::read_dir(registry.join("_anchors"))
        .map(|entries| {
            let mut paths = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension() == Some("jsonl".as_ref()))
                .collect::<Vec<_>>();
            paths.sort();
            paths
        })
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| read_lines(&path).map(|lines| (path, lines)))
        .collect();
    let relations_path = registry.join("relations.jsonl");
    let cannot_path = registry.join("_escrow/cannot_link.jsonl");
    Ok(Model {
        registry_path,
        registry_id: text("id"),
        version_before: text("version"),
        mapping,
        relations: read_lines(&relations_path).map(|lines| (relations_path, lines)),
        cannot_link: Some((
            cannot_path.clone(),
            read_lines(&cannot_path).unwrap_or_default(),
        )),
        anchors,
        corrections_text: fs::read_to_string(registry.join(CORRECTIONS_FILE)).unwrap_or_default(),
    })
}

fn entry_str<'a>(entry: &'a Value, name: &str) -> &'a str {
    entry.get(name).and_then(Value::as_str).unwrap_or_default()
}

fn key_namespace(rule_id: &str) -> Option<String> {
    rule_id
        .strip_prefix("SESSION_KEY_")
        .map(str::to_ascii_lowercase)
}

fn memory_names(id: &str) -> [String; 2] {
    [id.to_string(), format!("registry:{id}")]
}

/// Every name registry memory can use for an id: the id itself, its
/// `registry:` form, and the keys and surfaces the memory keys are built from.
fn identity_names(model: &Model, id: &str) -> BTreeSet<String> {
    let mut names = memory_names(id).into_iter().collect::<BTreeSet<_>>();
    for entry in model.entries_of(id) {
        let input = entry_str(entry, "input");
        match key_namespace(entry_str(entry, "rule_id")) {
            Some(namespace) => names.insert(format!("{namespace}:{input}")),
            None => names.insert(format!("surface:{input}")),
        };
    }
    names
}

impl Model {
    fn entries_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Value> {
        self.mapping
            .values()
            .flatten()
            .filter(move |entry| entry_str(entry, "canonical_id") == id)
    }

    fn types_of(&self, id: &str) -> BTreeSet<String> {
        self.entries_of(id)
            .map(|entry| entry_str(entry, "canonical_type").to_string())
            .collect()
    }

    fn entry_count(&self) -> usize {
        self.mapping.values().map(Vec::len).sum()
    }
}

struct RulesView {
    single_valued_authoritative: BTreeSet<String>,
}

fn load_rules(registry: &Path, path: &Path) -> Result<RulesView, Refusal> {
    let (rules, _) = crate::entity::session::rules::parse_rules(path)?;
    let _ = registry;
    Ok(RulesView {
        single_valued_authoritative: rules
            .key_rules
            .iter()
            .filter(|(_, rule)| rule.authoritative && rule.single_valued)
            .map(|(namespace, _)| namespace.clone())
            .collect(),
    })
}

fn validate(
    registry: &Path,
    model: &Model,
    line: &MergeLine,
    rules: Option<&RulesView>,
) -> Result<String, Refusal> {
    for (role, id) in [("survivor", &line.survivor), ("retired", &line.retired)] {
        if model.entries_of(id).next().is_none() {
            return Err(refusal(
                registry,
                format!("{role} '{id}' is not a canonical id in this registry"),
                json!({"role": role, "id": id, "writes_performed": false}),
                "Name two canonical ids that exist",
            ));
        }
    }
    let survivor_types = model.types_of(&line.survivor);
    let retired_types = model.types_of(&line.retired);
    if survivor_types != retired_types {
        return Err(refusal(
            registry,
            "the two ids have different canonical types",
            json!({
                "survivor": line.survivor,
                "survivor_types": survivor_types,
                "retired": line.retired,
                "retired_types": retired_types,
                "writes_performed": false,
            }),
            "Only ids of one canonical_type merge",
        ));
    }
    if let Some((_, facts)) = &model.cannot_link {
        let survivor = identity_names(model, &line.survivor);
        let retired = identity_names(model, &line.retired);
        for fact in facts
            .iter()
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        {
            let left = entry_str(&fact, "left_key").to_string();
            let right = entry_str(&fact, "right_key").to_string();
            let crosses = (survivor.contains(&left) && retired.contains(&right))
                || (survivor.contains(&right) && retired.contains(&left));
            if crosses {
                return Err(refusal(
                    registry,
                    "registry memory holds a cannot-link between the two ids",
                    json!({
                        "survivor": line.survivor,
                        "retired": line.retired,
                        "cannot_link": fact,
                        "writes_performed": false,
                    }),
                    "Remove the cannot-link with a detach correction first",
                ));
            }
        }
    }
    let Some(rules) = rules else {
        return Ok("not_run".to_string());
    };
    let values = |id: &str| {
        let mut by_namespace = BTreeMap::<String, BTreeSet<String>>::new();
        for entry in model.entries_of(id) {
            if let Some(namespace) = key_namespace(entry_str(entry, "rule_id")) {
                by_namespace
                    .entry(namespace)
                    .or_default()
                    .insert(entry_str(entry, "input").to_string());
            }
        }
        by_namespace
    };
    let survivor_keys = values(&line.survivor);
    let retired_keys = values(&line.retired);
    for namespace in &rules.single_valued_authoritative {
        let (Some(left), Some(right)) = (survivor_keys.get(namespace), retired_keys.get(namespace))
        else {
            continue;
        };
        if left != right {
            return Err(refusal(
                registry,
                format!("the two ids carry different {namespace} values"),
                json!({
                    "namespace": namespace,
                    "survivor": line.survivor,
                    "survivor_values": left,
                    "retired": line.retired,
                    "retired_values": right,
                    "writes_performed": false,
                }),
                "Different authoritative keys mean different entities; do not merge",
            ));
        }
    }
    Ok("passed".to_string())
}

fn rewrite_ids(value: &mut Value, retired: &str, survivor: &str) -> bool {
    let mut changed = false;
    match value {
        Value::Object(map) => {
            for (_, inner) in map.iter_mut() {
                changed |= rewrite_ids(inner, retired, survivor);
            }
        }
        Value::Array(items) => {
            for item in items {
                changed |= rewrite_ids(item, retired, survivor);
            }
        }
        Value::String(text) if text == retired => {
            *text = survivor.to_string();
            changed = true;
        }
        _ => {}
    }
    changed
}

/// Apply one merge to the in-memory model.
fn apply_merge(
    registry: &Path,
    model: &mut Model,
    line: &MergeLine,
    key_check: String,
) -> Result<RegistryCorrectionSummary, Refusal> {
    let correction_id = line.correction_id();
    let closure_hash = line.closure_hash();
    let outcome_ref = line.outcome_ref(&model.registry_id);
    let authority = json!({
        "version": "canon.registry.correction.authority.v1",
        "outcome_ref": outcome_ref,
        "closure_hash": closure_hash,
        "correction_id": correction_id,
    });
    let correction_mark = json!({
        "op": "merge",
        "from": line.retired,
        "correction_id": correction_id,
    });
    let iri = format!("cmdrvl:{}", line.retired);

    let mut repointed_aliases = 0;
    let mut repointed_key_aliases = 0;
    let mut superseded = Vec::new();
    let mut first_file: Option<PathBuf> = None;
    let canonical_type = model
        .types_of(&line.retired)
        .into_iter()
        .next()
        .unwrap_or_default();
    for (path, entries) in &mut model.mapping {
        for entry in entries.iter_mut() {
            if entry_str(entry, "canonical_id") != line.retired {
                continue;
            }
            first_file.get_or_insert_with(|| path.clone());
            let is_key = key_namespace(entry_str(entry, "rule_id")).is_some();
            let input = entry_str(entry, "input").to_string();
            let Some(object) = entry.as_object_mut() else {
                continue;
            };
            object.insert("canonical_id".to_string(), json!(line.survivor));
            object.insert("correction".to_string(), correction_mark.clone());
            if input == line.retired || input == iri {
                // The retired id (and its IRI form) is the superseded alias.
                if let Some(previous) = object.remove("session_authority") {
                    object.insert("superseded_authority".to_string(), previous);
                }
                object.insert("rule_id".to_string(), json!(SUPERSEDED_ID_ALIAS));
                object.insert("session_authority".to_string(), authority.clone());
                superseded.push(input);
            } else if is_key {
                repointed_key_aliases += 1;
            } else {
                repointed_aliases += 1;
            }
        }
    }
    if !superseded.iter().any(|input| input == &line.retired) {
        let target = first_file.clone().ok_or_else(|| {
            refusal(
                registry,
                "the retired id has no mapping entry to re-point",
                json!({"retired": line.retired, "writes_performed": false}),
                "Name a retired id that exists",
            )
        })?;
        let mut alias = Map::new();
        alias.insert("input".to_string(), json!(line.retired));
        alias.insert("canonical_id".to_string(), json!(line.survivor));
        alias.insert("canonical_type".to_string(), json!(canonical_type));
        alias.insert("rule_id".to_string(), json!(SUPERSEDED_ID_ALIAS));
        alias.insert("session_authority".to_string(), authority.clone());
        alias.insert("correction".to_string(), correction_mark.clone());
        if let Some(entries) = model.mapping.get_mut(&target) {
            entries.push(Value::Object(alias));
        }
        superseded.push(line.retired.clone());
    }
    superseded.sort();

    // Relations that name the retired id.
    let mut repointed_relations = 0;
    let mut dropped_relations = 0;
    if let Some((_, lines)) = &mut model.relations {
        let mut kept = Vec::new();
        let mut facts = Vec::new();
        let mut touched = false;
        for text in lines.iter() {
            let Ok(mut fact) = serde_json::from_str::<Value>(text) else {
                kept.push(text.clone());
                continue;
            };
            let names_retired = fact.pointer("/subject/identity_id").and_then(Value::as_str)
                == Some(line.retired.as_str())
                || fact.pointer("/object/identity_id").and_then(Value::as_str)
                    == Some(line.retired.as_str());
            if !names_retired {
                kept.push(text.clone());
                continue;
            }
            touched = true;
            for pointer in ["/subject/identity_id", "/object/identity_id"] {
                if let Some(slot) = fact.pointer_mut(pointer)
                    && slot.as_str() == Some(line.retired.as_str())
                {
                    *slot = json!(line.survivor);
                }
            }
            let subject = fact.pointer("/subject/identity_id").and_then(Value::as_str);
            let object = fact.pointer("/object/identity_id").and_then(Value::as_str);
            if subject == object {
                dropped_relations += 1;
                continue;
            }
            repointed_relations += 1;
            let typed: crate::temporal::relation::DirectedRelationFact =
                serde_json::from_value(fact).map_err(|error| {
                    refusal(
                        registry,
                        format!("a relation line is not a valid relation fact: {error}"),
                        json!({"writes_performed": false}),
                        "Repair relations.jsonl, then rerun",
                    )
                })?;
            let finalized =
                crate::temporal::relation::finalize_relation(typed).map_err(|error| {
                    refusal(
                        registry,
                        format!("a re-pointed relation is invalid: {error}"),
                        json!({"writes_performed": false}),
                        "Repair relations.jsonl, then rerun",
                    )
                })?;
            facts.push(serde_json::to_string(&finalized).unwrap_or_default());
        }
        if touched {
            kept.extend(facts);
            kept.sort();
            kept.dedup();
            *lines = kept;
        }
    }

    // Memory that names the retired id.
    let mut repointed_memory = 0;
    if let Some((_, lines)) = &mut model.cannot_link {
        let retired = memory_names(&line.retired);
        let survivor = memory_names(&line.survivor);
        let mut out = Vec::new();
        for text in lines.iter() {
            let Ok(mut fact) = serde_json::from_str::<Value>(text) else {
                out.push(text.clone());
                continue;
            };
            let mut touched = false;
            for key in ["left_key", "right_key"] {
                let current = entry_str(&fact, key).to_string();
                if let Some(index) = retired.iter().position(|name| *name == current) {
                    fact[key] = json!(survivor[index]);
                    touched = true;
                }
            }
            if !touched {
                out.push(text.clone());
            } else if entry_str(&fact, "left_key") != entry_str(&fact, "right_key") {
                repointed_memory += 1;
                out.push(serde_json::to_string(&fact).unwrap_or_default());
            }
        }
        *lines = out;
    }
    for (_, lines) in &mut model.anchors {
        for text in lines.iter_mut() {
            let Ok(mut record) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            if rewrite_ids(&mut record, &line.retired, &line.survivor) {
                repointed_memory += 1;
                *text = serde_json::to_string(&record).unwrap_or_default();
            }
        }
    }

    Ok(RegistryCorrectionSummary {
        correction_id,
        op: "merge".to_string(),
        survivor: Some(line.survivor.clone()),
        retired: Some(line.retired.clone()),
        outcome_ref,
        closure_hash,
        key_check,
        repointed_aliases,
        repointed_key_aliases,
        repointed_relations,
        dropped_relations,
        repointed_memory,
        superseded_aliases: superseded,
        ..Default::default()
    })
}

fn jsonl_bytes(lines: &[String]) -> Vec<u8> {
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text.into_bytes()
}

fn base_moved(expected: &str, actual: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityRegistrySnapshot,
        message: "The registry directory is not the base this correction expected".to_string(),
        detail: json!({
            "reason": "base_moved",
            "expected": expected,
            "actual": actual,
            "writes_performed": false,
        }),
        next_command: Some(
            "Run canon registry identity, then rerun with the current base".to_string(),
        ),
    }
}

pub fn correct(request: RegistryCorrectRequest) -> Result<RegistryCorrectOutput, Refusal> {
    let registry = request.registry.clone();
    let lines = parse_lines(&registry, &request.from)?;
    let base_identity = registry_identity(&registry)?;
    if let Some(expected) = &request.expect_base
        && expected != &base_identity.content_hash
    {
        return Err(base_moved(expected, &base_identity.content_hash));
    }
    let rules = request
        .rules
        .as_deref()
        .map(|path| load_rules(&registry, path))
        .transpose()?;

    let applied = applied_correction_ids(&registry);
    let pending = lines
        .iter()
        .filter(|line| !applied.contains(&line.correction_id()))
        .cloned()
        .collect::<Vec<_>>();
    let mut model = load_model(&registry)?;
    let version_before = model.version_before.clone();
    if pending.is_empty() {
        return Ok(RegistryCorrectOutput {
            version: REGISTRY_CORRECT_VERSION.to_string(),
            outcome: "already_applied".to_string(),
            registry_id: model.registry_id.clone(),
            version_before: version_before.clone(),
            version_after: version_before,
            corrections: Vec::new(),
            base: base_identity.side(),
            new: base_identity.side(),
            changed_files: Vec::new(),
        });
    }

    let original_files = model
        .mapping
        .iter()
        .map(|(path, rows)| (path.clone(), rows.clone()))
        .collect::<BTreeMap<_, _>>();
    let original_relations = model.relations.clone();
    let original_cannot = model.cannot_link.clone();
    let original_anchors = model.anchors.clone();

    let mut summaries = Vec::new();
    for correction in &pending {
        let ctx = ops::Ctx {
            correction_id: correction.correction_id(),
            closure_hash: correction.closure_hash(),
            outcome_ref: correction.outcome_ref(&model.registry_id),
        };
        summaries.push(match correction {
            Correction::Merge(line) => {
                let key_check = validate(&registry, &model, line, rules.as_ref())?;
                apply_merge(&registry, &mut model, line, key_check)?
            }
            Correction::Detach(line) => ops::apply_detach(&registry, &mut model, line, &ctx)?,
            Correction::Supersede(line) => supersede::apply(&registry, &mut model, line, &ctx)?,
            Correction::Split(line) => ops::apply_split(&registry, &mut model, line, &ctx)?,
            Correction::RemoveCannotLink(line) => {
                ops::apply_remove_cannot_link(&registry, &mut model, line, &ctx)?
            }
        });
    }

    let version_after = resolve_next_version(
        &registry,
        &version_before,
        request.bump,
        request.next_version.as_deref(),
    )?;
    let mut writes = BTreeMap::<PathBuf, Vec<u8>>::new();
    for (path, rows) in &model.mapping {
        if original_files.get(path) != Some(rows) {
            writes.insert(path.clone(), to_pretty_bytes(rows, &registry)?);
        }
    }
    if let Some((path, lines)) = &model.relations
        && original_relations.as_ref().map(|(_, old)| old) != Some(lines)
    {
        writes.insert(path.clone(), jsonl_bytes(lines));
    }
    if let Some((path, lines)) = &model.cannot_link
        && original_cannot.as_ref().map(|(_, old)| old) != Some(lines)
    {
        writes.insert(path.clone(), jsonl_bytes(lines));
    }
    for ((path, lines), (_, old)) in model.anchors.iter().zip(original_anchors.iter()) {
        if lines != old {
            writes.insert(path.clone(), jsonl_bytes(lines));
        }
    }
    writes.insert(
        model.registry_path.clone(),
        build_registry_bytes(
            &registry,
            &model.registry_path,
            &version_after,
            model.entry_count(),
        )?,
    );

    // File hashes the correction line records: every changed file except the
    // corrections file itself, which holds this record.
    let before_hashes = |path: &Path| fs::read(path).ok().map(|bytes| hash_bytes(&bytes));
    let recorded_files = writes
        .iter()
        .map(|(path, bytes)| {
            json!({
                "path": path
                    .strip_prefix(&registry)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                "blake3_before": before_hashes(path),
                "blake3_after": hash_bytes(bytes),
            })
        })
        .collect::<Vec<_>>();
    let mut corrections_text = model.corrections_text.clone();
    if !corrections_text.is_empty() && !corrections_text.ends_with('\n') {
        corrections_text.push('\n');
    }
    for (correction, summary) in pending.iter().zip(&summaries) {
        let (reviewer, evidence_ids, reason, source) = correction.review();
        let mut stored = json!({
            "version": REGISTRY_CORRECTION_LINE_VERSION,
            "correction_id": summary.correction_id,
            "op": correction.op(),
            "reviewer": reviewer,
            "evidence_ids": evidence_ids,
            "reason": reason,
            "source": source,
            "outcome_ref": summary.outcome_ref,
            "closure_hash": summary.closure_hash,
            "version_before": version_before,
            "version_after": version_after,
            "changed_files": recorded_files,
        });
        if let Some(object) = stored.as_object_mut() {
            match correction {
                Correction::Merge(line) => {
                    object.insert("survivor".to_string(), json!(line.survivor));
                    object.insert("retired".to_string(), json!(line.retired));
                    object.insert("key_check".to_string(), json!(summary.key_check));
                    object.insert(
                        "repointed".to_string(),
                        json!({
                            "aliases": summary.repointed_aliases,
                            "key_aliases": summary.repointed_key_aliases,
                            "relations": summary.repointed_relations,
                            "dropped_relations": summary.dropped_relations,
                            "memory": summary.repointed_memory,
                        }),
                    );
                }
                _ => {
                    if let Some(extra) = summary.stored.as_object() {
                        for (key, value) in extra {
                            object.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
        }
        corrections_text.push_str(&serde_json::to_string(&stored).unwrap_or_default());
        corrections_text.push('\n');
    }
    writes.insert(
        registry.join(CORRECTIONS_FILE),
        corrections_text.into_bytes(),
    );

    if request.dry_run {
        let changed = writes
            .iter()
            .map(|(path, bytes)| RegistryChangedFile {
                path: path
                    .strip_prefix(&registry)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                change: if path.exists() { "modified" } else { "added" }.to_string(),
                blake3_before: before_hashes(path),
                blake3_after: Some(hash_bytes(bytes)),
            })
            .collect();
        return Ok(RegistryCorrectOutput {
            version: REGISTRY_CORRECT_VERSION.to_string(),
            outcome: "dry_run".to_string(),
            registry_id: model.registry_id.clone(),
            version_before,
            version_after,
            corrections: summaries,
            base: base_identity.side(),
            new: base_identity.side(),
            changed_files: changed,
        });
    }

    let _guard =
        acquire_registry_mutation_guard(&registry).map_err(|error| io_refusal(&registry, error))?;
    let mut originals = BTreeMap::<PathBuf, Option<Vec<u8>>>::new();
    for path in writes.keys() {
        originals.insert(path.clone(), fs::read(path).ok());
    }
    let restore = |originals: &BTreeMap<PathBuf, Option<Vec<u8>>>| -> Result<(), Refusal> {
        for (path, bytes) in originals {
            match bytes {
                Some(bytes) => {
                    write_atomic(path, bytes).map_err(|error| io_refusal(path, error))?;
                }
                None => {
                    let _ = fs::remove_file(path);
                }
            }
        }
        Ok(())
    };
    for (path, bytes) in &writes {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| io_refusal(parent, error))?;
        }
        if let Err(error) = write_atomic(path, bytes) {
            restore(&originals)?;
            return Err(io_refusal(path, error));
        }
    }
    match crate::registry_lint::lint(&registry, RegistryLintProfile::Standard) {
        Ok(lint) if lint.summary.errors == 0 => {}
        Ok(lint) => {
            restore(&originals)?;
            let errors = lint
                .findings
                .iter()
                .filter(|finding| finding.severity == RegistryLintSeverity::Error)
                .map(|finding| finding.code.clone())
                .collect::<Vec<_>>();
            return Err(refusal(
                &registry,
                "Registry lint failed after the proposed correction; every file was restored",
                json!({"lint_errors": errors, "lint": lint, "writes_performed": false}),
                "Fix the registry or the correction, then rerun",
            ));
        }
        Err(refusal) => {
            restore(&originals)?;
            return Err(refusal);
        }
    }

    let new_identity = registry_identity(&registry)?;
    Ok(RegistryCorrectOutput {
        version: REGISTRY_CORRECT_VERSION.to_string(),
        outcome: "applied".to_string(),
        registry_id: model.registry_id.clone(),
        version_before,
        version_after,
        corrections: summaries,
        base: base_identity.side(),
        new: new_identity.side(),
        changed_files: changed_files(&base_identity.files, &new_identity.files),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(survivor: &str, retired: &str) -> MergeLine {
        MergeLine {
            survivor: survivor.to_string(),
            retired: retired.to_string(),
            reviewer: "human:zac".to_string(),
            evidence_ids: vec!["evidence:blake3:a".to_string()],
            reason: "same entity".to_string(),
            source: Value::Null,
        }
    }

    fn model(entries: Vec<Value>, relations: Vec<Value>, cannot: Vec<Value>) -> Model {
        Model {
            registry_path: PathBuf::from("registry.json"),
            registry_id: "orgs".to_string(),
            version_before: "1.0.0".to_string(),
            mapping: BTreeMap::from([(PathBuf::from("aliases.json"), entries)]),
            relations: Some((
                PathBuf::from("relations.jsonl"),
                relations.iter().map(Value::to_string).collect(),
            )),
            cannot_link: Some((
                PathBuf::from("cannot_link.jsonl"),
                cannot.iter().map(Value::to_string).collect(),
            )),
            anchors: Vec::new(),
            corrections_text: String::new(),
        }
    }

    fn entry(input: &str, id: &str, rule: &str) -> Value {
        json!({"input": input, "canonical_id": id, "canonical_type": "firm", "rule_id": rule})
    }

    #[test]
    fn the_closure_depends_only_on_what_the_reviewer_wrote() {
        let one = line("ORG-1", "ORG-2");
        let mut other = line("ORG-1", "ORG-2");
        assert_eq!(one.closure_hash(), other.closure_hash());
        other.evidence_ids.push("evidence:blake3:b".to_string());
        assert_ne!(one.closure_hash(), other.closure_hash());
        assert!(one.correction_id().starts_with("correction:"));
    }

    #[test]
    fn a_merge_re_points_every_entry_and_supersedes_the_retired_id() {
        let mut model = model(
            vec![
                entry("Acme", "ORG-1", "SESSION_TIER_A"),
                entry("Acme Corp", "ORG-2", "SESSION_TIER_A"),
                entry("1234", "ORG-2", "SESSION_KEY_CIK"),
            ],
            Vec::new(),
            Vec::new(),
        );
        let summary = apply_merge(
            Path::new("."),
            &mut model,
            &line("ORG-1", "ORG-2"),
            "not_run".to_string(),
        )
        .unwrap();
        let rows = &model.mapping[Path::new("aliases.json")];
        assert!(
            rows.iter()
                .all(|row| entry_str(row, "canonical_id") == "ORG-1")
        );
        assert_eq!(summary.repointed_aliases, 1);
        assert_eq!(summary.repointed_key_aliases, 1);
        let superseded = rows
            .iter()
            .filter(|row| entry_str(row, "rule_id") == SUPERSEDED_ID_ALIAS)
            .collect::<Vec<_>>();
        assert_eq!(superseded.len(), 1, "exactly one SUPERSEDED_ID_ALIAS");
        assert_eq!(entry_str(superseded[0], "input"), "ORG-2");
        assert_eq!(
            superseded[0]["session_authority"]["closure_hash"],
            json!(summary.closure_hash)
        );
        let moved = rows
            .iter()
            .find(|row| entry_str(row, "input") == "Acme Corp")
            .unwrap();
        assert_eq!(moved["correction"]["from"], "ORG-2");
        assert_eq!(
            entry_str(moved, "rule_id"),
            "SESSION_TIER_A",
            "rule_id kept"
        );
    }

    #[test]
    fn an_existing_retired_id_entry_becomes_the_superseded_alias() {
        let mut model = model(
            vec![
                entry("ORG-1", "ORG-1", "ID"),
                entry("ORG-2", "ORG-2", "ID"),
                entry("cmdrvl:ORG-2", "ORG-2", "IRI"),
            ],
            Vec::new(),
            Vec::new(),
        );
        let summary = apply_merge(
            Path::new("."),
            &mut model,
            &line("ORG-1", "ORG-2"),
            "not_run".to_string(),
        )
        .unwrap();
        assert_eq!(summary.superseded_aliases, ["ORG-2", "cmdrvl:ORG-2"]);
        let rows = &model.mapping[Path::new("aliases.json")];
        assert_eq!(rows.len(), 3, "no duplicate entry for the retired id");
    }

    #[test]
    fn a_relation_that_becomes_self_referential_is_dropped_and_reported() {
        let relation = |subject: &str, object: &str| {
            json!({
                "subject": {"identity_id": subject},
                "object": {"identity_id": object},
            })
        };
        let mut model = model(
            vec![entry("A", "ORG-1", "X"), entry("B", "ORG-2", "X")],
            vec![relation("ORG-1", "ORG-2"), relation("ORG-3", "ORG-4")],
            Vec::new(),
        );
        let summary = apply_merge(
            Path::new("."),
            &mut model,
            &line("ORG-1", "ORG-2"),
            "not_run".to_string(),
        )
        .unwrap();
        assert_eq!(summary.dropped_relations, 1);
        assert_eq!(summary.repointed_relations, 0);
        let (_, lines) = model.relations.as_ref().unwrap();
        assert_eq!(lines.len(), 1, "only the unrelated relation remains");
    }

    #[test]
    fn memory_naming_the_retired_id_is_re_pointed() {
        let mut model = model(
            vec![entry("A", "ORG-1", "X"), entry("B", "ORG-2", "X")],
            Vec::new(),
            vec![
                json!({"left_key": "registry:ORG-2", "right_key": "lei:999", "reason": "distinct"}),
                json!({"left_key": "lei:1", "right_key": "lei:2", "reason": "distinct"}),
            ],
        );
        let summary = apply_merge(
            Path::new("."),
            &mut model,
            &line("ORG-1", "ORG-2"),
            "not_run".to_string(),
        )
        .unwrap();
        assert_eq!(summary.repointed_memory, 1);
        let (_, lines) = model.cannot_link.as_ref().unwrap();
        assert!(lines[0].contains("registry:ORG-1"));
        assert!(lines[1].contains("lei:1"));
    }

    #[test]
    fn a_cannot_link_between_the_two_ids_refuses_the_merge() {
        let model = model(
            vec![entry("A", "ORG-1", "X"), entry("B", "ORG-2", "X")],
            Vec::new(),
            vec![
                json!({"left_key": "registry:ORG-1", "right_key": "registry:ORG-2", "reason": "distinct"}),
            ],
        );
        let error = validate(Path::new("."), &model, &line("ORG-1", "ORG-2"), None).unwrap_err();
        assert!(error.message.contains("cannot-link"), "{}", error.message);
    }

    #[test]
    fn different_authoritative_keys_refuse_naming_both_values() {
        let model = model(
            vec![
                entry("111", "ORG-1", "SESSION_KEY_CIK"),
                entry("222", "ORG-2", "SESSION_KEY_CIK"),
                entry("A", "ORG-1", "X"),
                entry("B", "ORG-2", "X"),
            ],
            Vec::new(),
            Vec::new(),
        );
        let rules = RulesView {
            single_valued_authoritative: BTreeSet::from(["cik".to_string()]),
        };
        let error = validate(
            Path::new("."),
            &model,
            &line("ORG-1", "ORG-2"),
            Some(&rules),
        )
        .unwrap_err();
        assert_eq!(error.detail["survivor_values"], json!(["111"]));
        assert_eq!(error.detail["retired_values"], json!(["222"]));
        // Without rules the check does not run.
        assert_eq!(
            validate(Path::new("."), &model, &line("ORG-1", "ORG-2"), None).unwrap(),
            "not_run"
        );
    }

    #[test]
    fn types_must_match_and_both_ids_must_exist() {
        let mut other_type = entry("B", "ORG-2", "X");
        other_type["canonical_type"] = json!("person");
        let model = model(
            vec![entry("A", "ORG-1", "X"), other_type],
            Vec::new(),
            Vec::new(),
        );
        assert!(validate(Path::new("."), &model, &line("ORG-1", "ORG-2"), None).is_err());
        assert!(validate(Path::new("."), &model, &line("ORG-1", "ORG-9"), None).is_err());
    }
}
