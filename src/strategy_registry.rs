use crate::{Refusal, RefusalCode, RegistryMeta};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const STRATEGY_DIR: &str = "_strategy";
const DEFAULT_ENTRIES_FILE: &str = "entries.json";
const DEFAULT_RULE_ID: &str = "STRATEGY_CHAMPION";
const STRATEGY_ENTRY_SCHEMA_VERSION: &str = "canon_strategy_entry.v1";

type StrategyResult<T> = Result<T, Refusal>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cardinality: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategySchemaShape {
    pub columns: Vec<StrategyColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenScript {
    pub id: String,
    pub path: String,
    pub language: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProofReference {
    pub path: String,
    pub content_hash: String,
    pub decision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProofs {
    pub verify: StrategyProofReference,
    pub assess: StrategyProofReference,
    pub airlock: StrategyProofReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyAttestationGrade {
    OperatorAttested,
    ProofAttested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyEntryStatus {
    Active,
    Deprecated,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StrategyEntryKey {
    Schema {
        schema_fingerprint: String,
        skill_hash: String,
    },
    Task {
        task: String,
        skill_hash: String,
    },
}

impl StrategyEntryKey {
    pub fn skill_hash(&self) -> &str {
        match self {
            Self::Schema { skill_hash, .. } | Self::Task { skill_hash, .. } => skill_hash,
        }
    }

    pub fn key_type(&self) -> &'static str {
        match self {
            Self::Schema { .. } => "schema",
            Self::Task { .. } => "task",
        }
    }

    fn schema_fingerprint(&self) -> Option<&str> {
        match self {
            Self::Schema {
                schema_fingerprint, ..
            } => Some(schema_fingerprint),
            Self::Task { .. } => None,
        }
    }

    fn task(&self) -> Option<&str> {
        match self {
            Self::Task { task, .. } => Some(task),
            Self::Schema { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyOperatorAttestation {
    pub operator: String,
    pub attested_at: String,
    pub reason: String,
    pub script_content_hash: String,
    pub attestation_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDeprecation {
    pub operator: String,
    pub deprecated_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyRegistryEntry {
    pub entry_schema_version: String,
    pub key: StrategyEntryKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<StrategySchemaShape>,
    pub grade: StrategyAttestationGrade,
    pub status: StrategyEntryStatus,
    pub skill_hash: String,
    pub script: FrozenScript,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proofs: Option<StrategyProofs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_attestation: Option<StrategyOperatorAttestation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<StrategyDeprecation>,
    pub rule_id: String,
}

impl StrategyRegistryEntry {
    pub fn schema_fingerprint(&self) -> Option<&str> {
        self.key.schema_fingerprint()
    }

    pub fn task(&self) -> Option<&str> {
        self.key.task()
    }

    pub fn is_active(&self) -> bool {
        self.status == StrategyEntryStatus::Active
    }
}

struct BuiltEntryKey {
    key: StrategyEntryKey,
    schema: Option<StrategySchemaShape>,
    schema_fingerprint: Option<String>,
    task: Option<String>,
    skill_hash: String,
}

struct AttestationInput<'a> {
    grade: StrategyAttestationGrade,
    script_hash: &'a str,
    operator: Option<&'a str>,
    reason: Option<&'a str>,
    attested_at: Option<&'a str>,
    verify_path: Option<&'a Path>,
    assess_path: Option<&'a Path>,
    airlock_path: Option<&'a Path>,
}

struct ReceiptInput {
    operation: String,
    next_version: String,
    before_registry_hash: String,
    after_registry_hash: String,
    key: StrategyEntryKey,
    grade_before: Option<StrategyAttestationGrade>,
    grade_after: StrategyAttestationGrade,
    status_before: Option<StrategyEntryStatus>,
    status_after: StrategyEntryStatus,
    script: FrozenScript,
    entry_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyResolveOutcome {
    Exact,
    Compatible,
    Partial,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyMatchTier {
    Exact,
    Compatible,
    Partial,
}

impl StrategyMatchTier {
    fn rank(self) -> u8 {
        match self {
            Self::Exact => 0,
            Self::Compatible => 1,
            Self::Partial => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTypeMismatch {
    pub column: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyCardinalityDifference {
    pub column: String,
    pub registered: Option<u64>,
    pub query: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyMatchDiagnostics {
    pub shared_columns: Vec<String>,
    pub missing_from_query: Vec<String>,
    pub extra_in_query: Vec<String>,
    pub type_mismatches: Vec<StrategyTypeMismatch>,
    pub cardinality_differences: Vec<StrategyCardinalityDifference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyCandidate {
    pub tier: StrategyMatchTier,
    pub key: StrategyEntryKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_fingerprint: Option<String>,
    pub grade: StrategyAttestationGrade,
    pub status: StrategyEntryStatus,
    pub source_file: String,
    pub entry_order: usize,
    pub rule_id: String,
    pub script: FrozenScript,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<StrategyMatchDiagnostics>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyQuery {
    pub key: StrategyEntryKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub skill_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyEscalation {
    pub reason: String,
    pub next_action: String,
    pub best_candidate: Option<StrategyCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyResolveOutput {
    pub version: String,
    pub outcome: StrategyResolveOutcome,
    pub registry: RegistryMeta,
    pub query: StrategyQuery,
    #[serde(rename = "match")]
    pub resolved_match: Option<StrategyCandidate>,
    pub escalation: Option<StrategyEscalation>,
    pub candidates_considered: usize,
}

impl StrategyResolveOutput {
    pub fn exit_code(&self) -> u8 {
        match self.outcome {
            StrategyResolveOutcome::Exact | StrategyResolveOutcome::Compatible => 0,
            StrategyResolveOutcome::Partial | StrategyResolveOutcome::Unresolved => 1,
        }
    }

    pub fn render_summary(&self) -> String {
        match &self.resolved_match {
            Some(resolved) => format!(
                "{}@{} strategy {:?} script={} schema={} skill={}",
                self.registry.id,
                self.registry.version,
                self.outcome,
                resolved.script.id,
                self.query
                    .schema_fingerprint
                    .as_deref()
                    .or(self.query.task.as_deref())
                    .unwrap_or("<unknown>"),
                self.query.skill_hash,
            ),
            None => format!(
                "{}@{} strategy {:?} key={} skill={} candidates={}",
                self.registry.id,
                self.registry.version,
                self.outcome,
                self.query
                    .schema_fingerprint
                    .as_deref()
                    .or(self.query.task.as_deref())
                    .unwrap_or("<unknown>"),
                self.query.skill_hash,
                self.candidates_considered,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyRegisteredEntry {
    pub key: StrategyEntryKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub skill_hash: String,
    pub grade: StrategyAttestationGrade,
    pub status: StrategyEntryStatus,
    pub script: FrozenScript,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyRegisterOutput {
    pub version: String,
    pub registry: RegistryMeta,
    pub entry_count: usize,
    pub registered: StrategyRegisteredEntry,
    pub receipt: StrategyMutationReceipt,
}

impl StrategyRegisterOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{}@{} registered strategy script={} schema={} skill={} entries={}",
            self.registry.id,
            self.registry.version,
            self.registered.script.id,
            match &self.registered.key {
                StrategyEntryKey::Schema {
                    schema_fingerprint, ..
                } => schema_fingerprint.as_str(),
                StrategyEntryKey::Task { task, .. } => task.as_str(),
            },
            self.registered.skill_hash,
            self.entry_count,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyLifecycleOutput {
    pub version: String,
    pub operation: String,
    pub registry: RegistryMeta,
    pub entry_count: usize,
    pub entry: StrategyRegisteredEntry,
    pub receipt: StrategyMutationReceipt,
}

impl StrategyLifecycleOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{}@{} {} strategy key={:?} script={} entries={}",
            self.registry.id,
            self.registry.version,
            self.operation,
            self.entry.key,
            self.entry.script.id,
            self.entry_count,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyCatalogEntry {
    pub key: StrategyEntryKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub skill_hash: String,
    pub grade: StrategyAttestationGrade,
    pub status: StrategyEntryStatus,
    pub script: FrozenScript,
    pub rule_id: String,
    pub source_file: String,
    pub entry_order: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyCatalogOutput {
    pub version: String,
    pub registry: RegistryMeta,
    pub entries: Vec<StrategyCatalogEntry>,
}

impl StrategyCatalogOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{}@{} strategy entries={}",
            self.registry.id,
            self.registry.version,
            self.entries.len(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyExplainOutput {
    pub version: String,
    pub registry: RegistryMeta,
    pub query: StrategyQuery,
    pub active_resolution: Option<StrategyCatalogEntry>,
    pub ignored: Vec<StrategyCatalogEntry>,
    pub next_command: String,
}

impl StrategyExplainOutput {
    pub fn render_summary(&self) -> String {
        let status = if self.active_resolution.is_some() {
            "active"
        } else {
            "unresolved"
        };
        format!(
            "{}@{} strategy explain {status} ignored={}",
            self.registry.id,
            self.registry.version,
            self.ignored.len(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProofHashes {
    pub verify: String,
    pub assess: String,
    pub airlock: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDiffEntry {
    pub key: StrategyEntryKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<StrategySchemaShape>,
    pub grade: StrategyAttestationGrade,
    pub status: StrategyEntryStatus,
    pub script: FrozenScript,
    pub proof_hashes: StrategyProofHashes,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_attestation_hash: Option<String>,
    pub rule_id: String,
    pub source_file: String,
    pub entry_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDiffValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<StrategySchemaShape>,
    pub grade: StrategyAttestationGrade,
    pub status: StrategyEntryStatus,
    pub script: FrozenScript,
    pub proof_hashes: StrategyProofHashes,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_attestation_hash: Option<String>,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyDiffChangeType {
    ScriptIdChange,
    ScriptPathChange,
    ScriptLanguageChange,
    ScriptContentHashChange,
    ProofHashChange,
    SchemaShapeChange,
    GradeChange,
    StatusChange,
    AttestationChange,
    RuleIdChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDiffChangedEntry {
    pub key: StrategyEntryKey,
    pub old: StrategyDiffValue,
    pub new: StrategyDiffValue,
    pub change_types: Vec<StrategyDiffChangeType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDiffSummary {
    pub total_old: usize,
    pub total_new: usize,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDiffOutput {
    pub version: String,
    pub old: RegistryMeta,
    pub new: RegistryMeta,
    pub summary: StrategyDiffSummary,
    pub added: Vec<StrategyDiffEntry>,
    pub removed: Vec<StrategyDiffEntry>,
    pub changed: Vec<StrategyDiffChangedEntry>,
    pub unchanged: Vec<StrategyDiffEntry>,
}

impl StrategyDiffOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{}: {} -> {} strategy diff | +{} added, -{} removed, ~{} changed, ={} unchanged",
            self.old.id,
            self.old.version,
            self.new.version,
            self.summary.added,
            self.summary.removed,
            self.summary.changed,
            self.summary.unchanged,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StrategyKeySelector<'a> {
    Schema(&'a Path),
    Task(&'a str),
}

pub struct StrategyResolveRequest<'a> {
    pub registry_dir: &'a Path,
    pub key: StrategyKeySelector<'a>,
    pub skill_path: Option<&'a Path>,
    pub skill_hash: Option<&'a str>,
}

pub struct StrategyRegisterRequest<'a> {
    pub registry_dir: &'a Path,
    pub key: StrategyKeySelector<'a>,
    pub skill_path: Option<&'a Path>,
    pub skill_hash: Option<&'a str>,
    pub script_path: &'a Path,
    pub script_id: &'a str,
    pub language: &'a str,
    pub grade: StrategyAttestationGrade,
    pub operator: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub attested_at: Option<&'a str>,
    pub verify_path: Option<&'a Path>,
    pub assess_path: Option<&'a Path>,
    pub airlock_path: Option<&'a Path>,
    pub next_version: &'a str,
    pub rule_id: Option<&'a str>,
}

pub struct StrategyLifecycleRequest<'a> {
    pub registry_dir: &'a Path,
    pub key: StrategyKeySelector<'a>,
    pub skill_path: Option<&'a Path>,
    pub skill_hash: Option<&'a str>,
    pub script_path: Option<&'a Path>,
    pub script_id: Option<&'a str>,
    pub language: Option<&'a str>,
    pub operator: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub attested_at: Option<&'a str>,
    pub verify_path: Option<&'a Path>,
    pub assess_path: Option<&'a Path>,
    pub airlock_path: Option<&'a Path>,
    pub next_version: &'a str,
}

pub struct StrategyCatalogRequest<'a> {
    pub registry_dir: &'a Path,
    pub key_type: Option<&'a str>,
    pub grade: Option<StrategyAttestationGrade>,
    pub status: Option<StrategyEntryStatus>,
}

pub struct StrategyExplainRequest<'a> {
    pub registry_dir: &'a Path,
    pub key: StrategyKeySelector<'a>,
    pub skill_path: Option<&'a Path>,
    pub skill_hash: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyMutationReceipt {
    pub operation: String,
    pub registry_id: String,
    pub before_version: String,
    pub after_version: String,
    pub before_registry_hash: String,
    pub after_registry_hash: String,
    pub key: StrategyEntryKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grade_before: Option<StrategyAttestationGrade>,
    pub grade_after: StrategyAttestationGrade,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_before: Option<StrategyEntryStatus>,
    pub status_after: StrategyEntryStatus,
    pub script: FrozenScript,
    pub source_file: String,
    pub entry_order: usize,
    pub next_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryJson {
    id: String,
    version: String,
    description: String,
    updated: String,
    entry_count: usize,
}

struct LoadedStrategyRegistry {
    registry_json: RegistryJson,
    meta: RegistryMeta,
    entries: Vec<StrategyEntryRecord>,
}

#[derive(Debug, Clone)]
struct StrategyEntryRecord {
    entry: StrategyRegistryEntry,
    source_file: String,
    entry_order: usize,
}

pub fn resolve(request: StrategyResolveRequest<'_>) -> StrategyResult<StrategyResolveOutput> {
    let registry = load_strategy_registry(request.registry_dir)?;
    let skill_hash = resolve_skill_hash(request.skill_path, request.skill_hash)?;

    match request.key {
        StrategyKeySelector::Schema(schema_path) => {
            resolve_schema_key(&registry, schema_path, skill_hash)
        }
        StrategyKeySelector::Task(task) => resolve_task_key(&registry, task, skill_hash),
    }
}

fn resolve_schema_key(
    registry: &LoadedStrategyRegistry,
    schema_path: &Path,
    skill_hash: String,
) -> StrategyResult<StrategyResolveOutput> {
    let schema = load_schema_shape(schema_path)?;
    let schema_fingerprint = fingerprint_schema(&schema)?;

    let mut candidates = registry
        .entries
        .iter()
        .filter(|record| record.entry.is_active())
        .filter(|record| record.entry.key.skill_hash() == skill_hash)
        .filter_map(|record| candidate_for(record, &schema))
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        left.tier
            .rank()
            .cmp(&right.tier.rank())
            .then_with(|| left.source_file.cmp(&right.source_file))
            .then_with(|| left.entry_order.cmp(&right.entry_order))
    });

    let candidates_considered = candidates.len();
    let best_candidate = candidates.first().cloned();
    let (outcome, resolved_match, escalation) = match best_candidate {
        Some(candidate) if candidate.tier == StrategyMatchTier::Exact => {
            (StrategyResolveOutcome::Exact, Some(candidate), None)
        }
        Some(candidate) if candidate.tier == StrategyMatchTier::Compatible => {
            (StrategyResolveOutcome::Compatible, Some(candidate), None)
        }
        Some(candidate) => (
            StrategyResolveOutcome::Partial,
            None,
            Some(StrategyEscalation {
                reason: "partial_schema_overlap".to_string(),
                next_action: "Escalate for script rewrite, then register the verified champion"
                    .to_string(),
                best_candidate: Some(candidate),
            }),
        ),
        None => (
            StrategyResolveOutcome::Unresolved,
            None,
            Some(StrategyEscalation {
                reason: "no_strategy_registry_match".to_string(),
                next_action: "Author a new script, run verify + assess + airlock, then register it"
                    .to_string(),
                best_candidate: None,
            }),
        ),
    };

    Ok(StrategyResolveOutput {
        version: "canon_strategy_resolve.v0".to_string(),
        outcome,
        registry: registry.meta.clone(),
        query: StrategyQuery {
            key: StrategyEntryKey::Schema {
                schema_fingerprint: schema_fingerprint.clone(),
                skill_hash: skill_hash.clone(),
            },
            schema_path: Some(schema_path.display().to_string()),
            schema_fingerprint: Some(schema_fingerprint),
            task: None,
            skill_hash,
            column_count: Some(schema.columns.len()),
        },
        resolved_match,
        escalation,
        candidates_considered,
    })
}

fn resolve_task_key(
    registry: &LoadedStrategyRegistry,
    task: &str,
    skill_hash: String,
) -> StrategyResult<StrategyResolveOutput> {
    let task = normalize_task(task)?;
    let key = StrategyEntryKey::Task {
        task: task.clone(),
        skill_hash: skill_hash.clone(),
    };
    let resolved_match = registry
        .entries
        .iter()
        .find(|record| record.entry.is_active() && record.entry.key == key)
        .map(task_candidate);
    let candidates_considered = usize::from(resolved_match.is_some());
    let (outcome, escalation) = if resolved_match.is_some() {
        (StrategyResolveOutcome::Exact, None)
    } else {
        (
            StrategyResolveOutcome::Unresolved,
            Some(StrategyEscalation {
                reason: "no_active_task_strategy_match".to_string(),
                next_action:
                    "Register an operator-attested task champion, or explain the task key first"
                        .to_string(),
                best_candidate: None,
            }),
        )
    };

    Ok(StrategyResolveOutput {
        version: "canon_strategy_resolve.v0".to_string(),
        outcome,
        registry: registry.meta.clone(),
        query: StrategyQuery {
            key,
            schema_path: None,
            schema_fingerprint: None,
            task: Some(task),
            skill_hash,
            column_count: None,
        },
        resolved_match,
        escalation,
        candidates_considered,
    })
}

pub fn register(request: StrategyRegisterRequest<'_>) -> StrategyResult<StrategyRegisterOutput> {
    let registry = load_strategy_registry(request.registry_dir)?;
    let before_registry_hash = hash_registry_state(request.registry_dir)?;
    if request.next_version == registry.registry_json.version {
        return Err(Refusal::strategy_version_bump_required(
            "Strategy registration requires a new registry version",
            json!({
                "registry": request.registry_dir.display().to_string(),
                "current_version": registry.registry_json.version,
                "next_version": request.next_version,
            }),
        ));
    }

    let built_key = build_entry_key(request.key, request.skill_path, request.skill_hash)?;

    if registry
        .entries
        .iter()
        .any(|record| record.entry.is_active() && record.entry.key == built_key.key)
    {
        return Err(Refusal::strategy_input_contract(
            "An active strategy entry already exists for this key and skill hash",
            json!({
                "registry": request.registry_dir.display().to_string(),
                "key": built_key.key,
            }),
        ));
    }

    let script_id = required_non_empty("script-id", request.script_id)?;
    let language = required_non_empty("language", request.language)?;
    let rule_id = required_non_empty("rule-id", request.rule_id.unwrap_or(DEFAULT_RULE_ID))?;
    let script_hash = hash_file(request.script_path)?;
    let script = FrozenScript {
        id: script_id.to_string(),
        path: request.script_path.display().to_string(),
        language: language.to_string(),
        content_hash: script_hash.clone(),
    };

    let (proofs, operator_attestation) = build_attestation(AttestationInput {
        grade: request.grade,
        script_hash: &script_hash,
        operator: request.operator,
        reason: request.reason,
        attested_at: request.attested_at,
        verify_path: request.verify_path,
        assess_path: request.assess_path,
        airlock_path: request.airlock_path,
    })?;

    let new_entry = StrategyRegistryEntry {
        entry_schema_version: STRATEGY_ENTRY_SCHEMA_VERSION.to_string(),
        key: built_key.key.clone(),
        schema: built_key.schema,
        grade: request.grade,
        status: StrategyEntryStatus::Active,
        skill_hash: built_key.skill_hash.clone(),
        script: script.clone(),
        proofs,
        operator_attestation,
        deprecation: None,
        rule_id: rule_id.to_string(),
    };

    let entry_order = append_strategy_entry(request.registry_dir, &new_entry)?;
    let entry_count = registry.entries.len() + 1;
    update_registry_json(
        request.registry_dir,
        registry.registry_json.clone(),
        request.next_version,
        entry_count,
    )?;
    let after_registry_hash = hash_registry_state(request.registry_dir)?;
    let receipt = mutation_receipt(
        &registry,
        ReceiptInput {
            operation: "register".to_string(),
            next_version: request.next_version.to_string(),
            before_registry_hash,
            after_registry_hash,
            key: built_key.key.clone(),
            grade_before: None,
            grade_after: request.grade,
            status_before: None,
            status_after: StrategyEntryStatus::Active,
            script: script.clone(),
            entry_order,
        },
    );

    Ok(StrategyRegisterOutput {
        version: "canon_strategy_register.v0".to_string(),
        registry: RegistryMeta {
            id: registry.meta.id,
            version: request.next_version.to_string(),
            source: registry.meta.source,
        },
        entry_count,
        registered: StrategyRegisteredEntry {
            key: built_key.key,
            schema_fingerprint: built_key.schema_fingerprint,
            task: built_key.task,
            skill_hash: built_key.skill_hash,
            grade: request.grade,
            status: StrategyEntryStatus::Active,
            script,
            rule_id: rule_id.to_string(),
        },
        receipt,
    })
}

pub fn update(request: StrategyLifecycleRequest<'_>) -> StrategyResult<StrategyLifecycleOutput> {
    mutate_strategy_entry(request, "update", |entry, request| {
        let script_path = request.script_path.ok_or_else(|| {
            Refusal::strategy_input_contract(
                "strategy update requires --script",
                json!({ "missing": "script" }),
            )
        })?;
        let script_id = required_non_empty("script-id", request.script_id.unwrap_or(""))?;
        let language = required_non_empty("language", request.language.unwrap_or(""))?;
        let script_hash = hash_file(script_path)?;
        let script = FrozenScript {
            id: script_id.to_string(),
            path: script_path.display().to_string(),
            language: language.to_string(),
            content_hash: script_hash.clone(),
        };
        let (proofs, operator_attestation) = build_attestation(AttestationInput {
            grade: entry.grade,
            script_hash: &script_hash,
            operator: request.operator,
            reason: request.reason,
            attested_at: request.attested_at,
            verify_path: request.verify_path,
            assess_path: request.assess_path,
            airlock_path: request.airlock_path,
        })?;
        entry.script = script;
        entry.proofs = proofs;
        entry.operator_attestation = operator_attestation;
        Ok(())
    })
}

pub fn deprecate(request: StrategyLifecycleRequest<'_>) -> StrategyResult<StrategyLifecycleOutput> {
    mutate_strategy_entry(request, "deprecate", |entry, request| {
        let operator = required_non_empty("operator", request.operator.unwrap_or(""))?;
        let reason = required_single_line_reason(request.reason.unwrap_or(""))?;
        let live_timestamp;
        let deprecated_at = match request.attested_at {
            Some(value) => required_non_empty("attested-at", value)?,
            None => {
                live_timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
                live_timestamp.as_str()
            }
        };
        entry.status = StrategyEntryStatus::Deprecated;
        entry.deprecation = Some(StrategyDeprecation {
            operator: operator.to_string(),
            deprecated_at: deprecated_at.to_string(),
            reason: reason.to_string(),
        });
        Ok(())
    })
}

pub fn promote(request: StrategyLifecycleRequest<'_>) -> StrategyResult<StrategyLifecycleOutput> {
    mutate_strategy_entry(request, "promote", |entry, request| {
        if entry.grade == StrategyAttestationGrade::ProofAttested {
            return Err(Refusal::strategy_input_contract(
                "strategy entry is already proof-attested",
                json!({ "key": entry.key }),
            ));
        }
        let verify_path = request
            .verify_path
            .ok_or_else(|| missing_proof_refusal("verify"))?;
        let assess_path = request
            .assess_path
            .ok_or_else(|| missing_proof_refusal("assess"))?;
        let airlock_path = request
            .airlock_path
            .ok_or_else(|| missing_proof_refusal("airlock"))?;
        entry.proofs = Some(StrategyProofs {
            verify: load_proof_reference(verify_path, ProofKind::Verify)?,
            assess: load_proof_reference(assess_path, ProofKind::Assess)?,
            airlock: load_proof_reference(airlock_path, ProofKind::Airlock)?,
        });
        entry.grade = StrategyAttestationGrade::ProofAttested;
        Ok(())
    })
}

fn mutate_strategy_entry<F>(
    request: StrategyLifecycleRequest<'_>,
    operation: &str,
    mutate: F,
) -> StrategyResult<StrategyLifecycleOutput>
where
    F: FnOnce(&mut StrategyRegistryEntry, &StrategyLifecycleRequest<'_>) -> StrategyResult<()>,
{
    let registry = load_strategy_registry(request.registry_dir)?;
    let before_registry_hash = hash_registry_state(request.registry_dir)?;
    if request.next_version == registry.registry_json.version {
        return Err(Refusal::strategy_version_bump_required(
            format!("Strategy {operation} requires a new registry version"),
            json!({
                "registry": request.registry_dir.display().to_string(),
                "current_version": registry.registry_json.version,
                "next_version": request.next_version,
            }),
        ));
    }
    let built_key = build_entry_key(request.key, request.skill_path, request.skill_hash)?;
    let mut matches = registry
        .entries
        .iter()
        .filter(|record| record.entry.is_active() && record.entry.key == built_key.key)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return Err(Refusal::strategy_input_contract(
            format!("No active strategy entry found for {operation}"),
            json!({ "key": built_key.key }),
        ));
    }
    if matches.len() > 1 {
        return Err(Refusal::strategy_input_contract(
            format!("Ambiguous duplicate active strategy entries for {operation}"),
            json!({ "key": built_key.key, "matches": matches.len() }),
        ));
    }
    let record = matches.remove(0);
    let mut entry = record.entry.clone();
    let grade_before = entry.grade;
    let status_before = entry.status;
    mutate(&mut entry, &request)?;
    let entry_order = rewrite_strategy_entry(
        request.registry_dir,
        &record.source_file,
        record.entry_order,
        &entry,
    )?;
    update_registry_json(
        request.registry_dir,
        registry.registry_json.clone(),
        request.next_version,
        registry.entries.len(),
    )?;
    let after_registry_hash = hash_registry_state(request.registry_dir)?;
    let receipt = mutation_receipt(
        &registry,
        ReceiptInput {
            operation: operation.to_string(),
            next_version: request.next_version.to_string(),
            before_registry_hash,
            after_registry_hash,
            key: entry.key.clone(),
            grade_before: Some(grade_before),
            grade_after: entry.grade,
            status_before: Some(status_before),
            status_after: entry.status,
            script: entry.script.clone(),
            entry_order,
        },
    );
    let registered = entry_to_registered(&entry);
    Ok(StrategyLifecycleOutput {
        version: "canon_strategy_lifecycle.v0".to_string(),
        operation: operation.to_string(),
        registry: RegistryMeta {
            id: registry.meta.id,
            version: request.next_version.to_string(),
            source: registry.meta.source,
        },
        entry_count: registry.entries.len(),
        entry: registered,
        receipt,
    })
}

pub fn list(request: StrategyCatalogRequest<'_>) -> StrategyResult<StrategyCatalogOutput> {
    let registry = load_strategy_registry(request.registry_dir)?;
    let entries = registry
        .entries
        .iter()
        .filter(|record| {
            request
                .key_type
                .is_none_or(|key_type| record.entry.key.key_type() == key_type)
        })
        .filter(|record| {
            request
                .grade
                .is_none_or(|grade| record.entry.grade == grade)
        })
        .filter(|record| {
            request
                .status
                .is_none_or(|status| record.entry.status == status)
        })
        .map(entry_to_catalog)
        .collect();
    Ok(StrategyCatalogOutput {
        version: "canon_strategy_list.v0".to_string(),
        registry: registry.meta,
        entries,
    })
}

pub fn explain(request: StrategyExplainRequest<'_>) -> StrategyResult<StrategyExplainOutput> {
    let registry = load_strategy_registry(request.registry_dir)?;
    let built_key = build_entry_key(request.key, request.skill_path, request.skill_hash)?;
    let matches = registry
        .entries
        .iter()
        .filter(|record| record.entry.key == built_key.key)
        .collect::<Vec<_>>();
    let active_resolution = matches
        .iter()
        .find(|record| record.entry.is_active())
        .map(|record| entry_to_catalog(record));
    let ignored = matches
        .iter()
        .filter(|record| !record.entry.is_active())
        .map(|record| entry_to_catalog(record))
        .collect::<Vec<_>>();
    let next_command = if active_resolution.is_some() {
        "canon strategy resolve --registry <DIR> with the same key selector".to_string()
    } else {
        "canon strategy register --registry <DIR> with the same key selector".to_string()
    };
    Ok(StrategyExplainOutput {
        version: "canon_strategy_explain.v0".to_string(),
        registry: registry.meta,
        query: StrategyQuery {
            key: built_key.key,
            schema_path: match request.key {
                StrategyKeySelector::Schema(path) => Some(path.display().to_string()),
                StrategyKeySelector::Task(_) => None,
            },
            schema_fingerprint: built_key.schema_fingerprint,
            task: built_key.task,
            skill_hash: built_key.skill_hash,
            column_count: built_key.schema.as_ref().map(|schema| schema.columns.len()),
        },
        active_resolution,
        ignored,
        next_command,
    })
}

fn build_entry_key(
    key: StrategyKeySelector<'_>,
    skill_path: Option<&Path>,
    skill_hash: Option<&str>,
) -> StrategyResult<BuiltEntryKey> {
    let skill_hash = resolve_skill_hash(skill_path, skill_hash)?;
    match key {
        StrategyKeySelector::Schema(schema_path) => {
            let schema = load_schema_shape(schema_path)?;
            let schema_fingerprint = fingerprint_schema(&schema)?;
            Ok(BuiltEntryKey {
                key: StrategyEntryKey::Schema {
                    schema_fingerprint: schema_fingerprint.clone(),
                    skill_hash: skill_hash.clone(),
                },
                schema: Some(schema),
                schema_fingerprint: Some(schema_fingerprint),
                task: None,
                skill_hash,
            })
        }
        StrategyKeySelector::Task(task) => {
            let task = normalize_task(task)?;
            Ok(BuiltEntryKey {
                key: StrategyEntryKey::Task {
                    task: task.clone(),
                    skill_hash: skill_hash.clone(),
                },
                schema: None,
                schema_fingerprint: None,
                task: Some(task),
                skill_hash,
            })
        }
    }
}

fn build_attestation(
    input: AttestationInput<'_>,
) -> StrategyResult<(Option<StrategyProofs>, Option<StrategyOperatorAttestation>)> {
    match input.grade {
        StrategyAttestationGrade::OperatorAttested => {
            let operator = required_non_empty("operator", input.operator.unwrap_or(""))?;
            let reason = required_single_line_reason(input.reason.unwrap_or(""))?;
            let live_timestamp;
            let attested_at = match input.attested_at {
                Some(value) => required_non_empty("attested-at", value)?,
                None => {
                    live_timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
                    live_timestamp.as_str()
                }
            };
            let attestation_hash = hash_bytes(
                serde_json::to_string(&json!({
                    "operator": operator,
                    "attested_at": attested_at,
                    "reason": reason,
                    "script_content_hash": input.script_hash,
                }))
                .expect("attestation hash input serializes")
                .as_bytes(),
            );
            Ok((
                None,
                Some(StrategyOperatorAttestation {
                    operator: operator.to_string(),
                    attested_at: attested_at.to_string(),
                    reason: reason.to_string(),
                    script_content_hash: input.script_hash.to_string(),
                    attestation_hash,
                }),
            ))
        }
        StrategyAttestationGrade::ProofAttested => {
            let verify_path = input
                .verify_path
                .ok_or_else(|| missing_proof_refusal("verify"))?;
            let assess_path = input
                .assess_path
                .ok_or_else(|| missing_proof_refusal("assess"))?;
            let airlock_path = input
                .airlock_path
                .ok_or_else(|| missing_proof_refusal("airlock"))?;
            Ok((
                Some(StrategyProofs {
                    verify: load_proof_reference(verify_path, ProofKind::Verify)?,
                    assess: load_proof_reference(assess_path, ProofKind::Assess)?,
                    airlock: load_proof_reference(airlock_path, ProofKind::Airlock)?,
                }),
                None,
            ))
        }
    }
}

fn missing_proof_refusal(proof: &str) -> Refusal {
    Refusal::strategy_proof_invalid(
        format!("proof-attested strategy registration requires --{proof}"),
        json!({ "missing": proof }),
    )
}

fn required_single_line_reason(value: &str) -> StrategyResult<&str> {
    let value = required_non_empty("reason", value)?;
    if value.contains('\n') || value.contains('\r') {
        Err(Refusal::strategy_input_contract(
            "reason must be a single line",
            json!({ "field": "reason" }),
        ))
    } else {
        Ok(value)
    }
}

fn normalize_task(task: &str) -> StrategyResult<String> {
    let task = task.trim_matches(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'));
    if task.is_empty() {
        Err(Refusal::strategy_input_contract(
            "task cannot be empty",
            json!({ "field": "task" }),
        ))
    } else {
        Ok(task.to_string())
    }
}

fn mutation_receipt(
    registry: &LoadedStrategyRegistry,
    input: ReceiptInput,
) -> StrategyMutationReceipt {
    let task_or_schema = match &input.key {
        StrategyEntryKey::Schema {
            schema_fingerprint, ..
        } => schema_fingerprint.clone(),
        StrategyEntryKey::Task { task, .. } => task.clone(),
    };
    StrategyMutationReceipt {
        operation: input.operation,
        registry_id: registry.meta.id.clone(),
        before_version: registry.registry_json.version.clone(),
        after_version: input.next_version,
        before_registry_hash: input.before_registry_hash,
        after_registry_hash: input.after_registry_hash,
        key: input.key,
        grade_before: input.grade_before,
        grade_after: input.grade_after,
        status_before: input.status_before,
        status_after: input.status_after,
        script: input.script,
        source_file: DEFAULT_ENTRIES_FILE.to_string(),
        entry_order: input.entry_order,
        next_commands: vec![
            format!("canon strategy resolve --registry <DIR> --task-or-schema {task_or_schema}"),
            "canon strategy list --registry <DIR>".to_string(),
            "canon registry lint <DIR> --profile strategy".to_string(),
        ],
    }
}

pub fn diff(old_dir: &Path, new_dir: &Path) -> StrategyResult<StrategyDiffOutput> {
    let old_registry = load_strategy_registry(old_dir)?;
    let new_registry = load_strategy_registry(new_dir)?;

    if old_registry.meta.id != new_registry.meta.id {
        return Err(Refusal::bad_registry(
            &new_dir.display().to_string(),
            &format!(
                "Cannot diff strategy registries with different ids: '{}' ({}) != '{}' ({})",
                old_dir.display(),
                old_registry.meta.id,
                new_dir.display(),
                new_registry.meta.id,
            ),
        ));
    }

    let old_entries = effective_strategy_entries(&old_registry);
    let new_entries = effective_strategy_entries(&new_registry);
    let mut keys = old_entries.keys().cloned().collect::<BTreeSet<_>>();
    keys.extend(new_entries.keys().cloned());

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();

    for key in keys {
        match (old_entries.get(&key), new_entries.get(&key)) {
            (None, Some(new_entry)) => added.push(diff_entry(new_entry)),
            (Some(old_entry), None) => removed.push(diff_entry(old_entry)),
            (Some(old_entry), Some(new_entry)) => {
                let old_value = diff_value(old_entry);
                let new_value = diff_value(new_entry);
                let change_types = classify_strategy_change(&old_value, &new_value);
                if change_types.is_empty() {
                    unchanged.push(diff_entry(old_entry));
                } else {
                    changed.push(StrategyDiffChangedEntry {
                        key,
                        old: old_value,
                        new: new_value,
                        change_types,
                    });
                }
            }
            (None, None) => {}
        }
    }

    Ok(StrategyDiffOutput {
        version: "canon_strategy_diff.v0".to_string(),
        old: old_registry.meta,
        new: new_registry.meta,
        summary: StrategyDiffSummary {
            total_old: old_entries.len(),
            total_new: new_entries.len(),
            added: added.len(),
            removed: removed.len(),
            changed: changed.len(),
            unchanged: unchanged.len(),
        },
        added,
        removed,
        changed,
        unchanged,
    })
}

fn load_strategy_registry(registry_dir: &Path) -> StrategyResult<LoadedStrategyRegistry> {
    if !registry_dir.exists() || !registry_dir.is_dir() {
        return Err(Refusal::bad_registry(
            &registry_dir.display().to_string(),
            "registry directory not found",
        ));
    }

    let registry_json_path = registry_dir.join("registry.json");
    let registry_json_content = fs::read_to_string(&registry_json_path).map_err(|error| {
        Refusal::bad_registry(
            &registry_dir.display().to_string(),
            &format!("failed to read registry.json: {error}"),
        )
    })?;
    let registry_json: RegistryJson =
        serde_json::from_str(&registry_json_content).map_err(|error| {
            Refusal::bad_registry(
                &registry_dir.display().to_string(),
                &format!("failed to parse registry.json: {error}"),
            )
        })?;
    let entries = discover_strategy_entries(registry_dir)?;
    if registry_json.entry_count != entries.len() {
        eprintln!(
            "Warning: registry.json entry_count ({}) differs from strategy entry count ({}). Update to \"entry_count\": {}",
            registry_json.entry_count,
            entries.len(),
            entries.len()
        );
    }

    Ok(LoadedStrategyRegistry {
        meta: RegistryMeta {
            id: registry_json.id.clone(),
            version: registry_json.version.clone(),
            source: registry_dir.to_string_lossy().into_owned(),
        },
        registry_json,
        entries,
    })
}

fn discover_strategy_entries(registry_dir: &Path) -> StrategyResult<Vec<StrategyEntryRecord>> {
    let strategy_dir = registry_dir.join(STRATEGY_DIR);
    if !strategy_dir.exists() {
        return Ok(Vec::new());
    }
    if !strategy_dir.is_dir() {
        return Err(Refusal::bad_registry(
            &registry_dir.display().to_string(),
            "_strategy exists but is not a directory",
        ));
    }

    let mut paths = fs::read_dir(&strategy_dir)
        .map_err(|error| {
            Refusal::bad_registry(
                &registry_dir.display().to_string(),
                &format!("failed to read _strategy directory: {error}"),
            )
        })?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            Refusal::bad_registry(
                &registry_dir.display().to_string(),
                &format!("failed to read _strategy entry: {error}"),
            )
        })?;

    paths.retain(|path| path.is_file() && path.extension() == Some("json".as_ref()));
    paths.sort();

    let mut records = Vec::new();
    for path in paths {
        let content = fs::read_to_string(&path).map_err(|error| {
            Refusal::bad_registry(
                &registry_dir.display().to_string(),
                &format!("failed to read strategy file '{}': {error}", path.display()),
            )
        })?;
        let entries: Vec<StrategyRegistryEntry> =
            serde_json::from_str(&content).map_err(|error| {
                Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!(
                        "failed to parse strategy file '{}': {error}",
                        path.display()
                    ),
                )
            })?;
        let source_file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        for (entry_order, entry) in entries.into_iter().enumerate() {
            validate_registry_entry(registry_dir, &path, entry_order, &entry)?;
            records.push(StrategyEntryRecord {
                entry,
                source_file: source_file.clone(),
                entry_order,
            });
        }
    }
    Ok(records)
}

fn validate_registry_entry(
    registry_dir: &Path,
    path: &Path,
    entry_order: usize,
    entry: &StrategyRegistryEntry,
) -> StrategyResult<()> {
    let context = json!({
        "registry": registry_dir.display().to_string(),
        "strategy_file": path.display().to_string(),
        "entry_order": entry_order,
    });
    if entry.entry_schema_version != STRATEGY_ENTRY_SCHEMA_VERSION {
        return Err(Refusal::bad_registry(
            &registry_dir.display().to_string(),
            &format!("invalid strategy entry schema version: {context}"),
        ));
    }
    if entry.skill_hash.is_empty()
        || entry.script.id.is_empty()
        || entry.script.path.is_empty()
        || entry.script.language.is_empty()
        || entry.script.content_hash.is_empty()
        || entry.rule_id.is_empty()
    {
        return Err(Refusal::bad_registry(
            &registry_dir.display().to_string(),
            &format!("invalid strategy entry metadata: {context}"),
        ));
    }
    if entry.key.skill_hash() != entry.skill_hash {
        return Err(Refusal::bad_registry(
            &registry_dir.display().to_string(),
            &format!("strategy entry key skill_hash mismatch: {context}"),
        ));
    }
    match &entry.key {
        StrategyEntryKey::Schema {
            schema_fingerprint, ..
        } => {
            let schema = entry.schema.as_ref().ok_or_else(|| {
                Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("schema-keyed strategy entry is missing schema: {context}"),
                )
            })?;
            if schema.columns.is_empty() || schema_fingerprint.is_empty() {
                return Err(Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("invalid schema-keyed strategy entry metadata: {context}"),
                ));
            }
            let actual_fingerprint = fingerprint_schema(schema)?;
            if actual_fingerprint != *schema_fingerprint {
                return Err(Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!(
                        "strategy entry schema_fingerprint mismatch in '{}': expected {}, actual {}",
                        path.display(),
                        schema_fingerprint,
                        actual_fingerprint
                    ),
                ));
            }
        }
        StrategyEntryKey::Task { task, .. } => {
            if task.trim().is_empty() {
                return Err(Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("task-keyed strategy entry is missing task: {context}"),
                ));
            }
        }
    }
    match entry.grade {
        StrategyAttestationGrade::OperatorAttested => {
            let attestation = entry.operator_attestation.as_ref().ok_or_else(|| {
                Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("operator-attested entry is missing attestation: {context}"),
                )
            })?;
            if attestation.operator.trim().is_empty()
                || attestation.attested_at.trim().is_empty()
                || attestation.reason.trim().is_empty()
                || attestation.script_content_hash.trim().is_empty()
                || attestation.attestation_hash.trim().is_empty()
            {
                return Err(Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("operator-attested entry has incomplete attestation: {context}"),
                ));
            }
        }
        StrategyAttestationGrade::ProofAttested => {
            let proofs = entry.proofs.as_ref().ok_or_else(|| {
                Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("proof-attested entry is missing proofs: {context}"),
                )
            })?;
            if proof_reference_is_incomplete(&proofs.verify)
                || proof_reference_is_incomplete(&proofs.assess)
                || proof_reference_is_incomplete(&proofs.airlock)
            {
                return Err(Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("proof-attested entry has incomplete proofs: {context}"),
                ));
            }
        }
    }
    if entry.status == StrategyEntryStatus::Deprecated && entry.deprecation.is_none() {
        return Err(Refusal::bad_registry(
            &registry_dir.display().to_string(),
            &format!("deprecated strategy entry is missing deprecation metadata: {context}"),
        ));
    }
    Ok(())
}

fn proof_reference_is_incomplete(proof: &StrategyProofReference) -> bool {
    proof.path.is_empty() || proof.content_hash.is_empty() || proof.decision.is_empty()
}

fn effective_strategy_entries(
    registry: &LoadedStrategyRegistry,
) -> BTreeMap<StrategyEntryKey, StrategyEntryRecord> {
    let mut entries = BTreeMap::new();
    for record in &registry.entries {
        entries
            .entry(strategy_entry_key(&record.entry))
            .or_insert_with(|| record.clone());
    }
    entries
}

fn strategy_entry_key(entry: &StrategyRegistryEntry) -> StrategyEntryKey {
    entry.key.clone()
}

fn proof_hashes(entry: &StrategyRegistryEntry) -> StrategyProofHashes {
    entry
        .proofs
        .as_ref()
        .map(|proofs| StrategyProofHashes {
            verify: proofs.verify.content_hash.clone(),
            assess: proofs.assess.content_hash.clone(),
            airlock: proofs.airlock.content_hash.clone(),
        })
        .unwrap_or_else(|| StrategyProofHashes {
            verify: String::new(),
            assess: String::new(),
            airlock: String::new(),
        })
}

fn diff_entry(record: &StrategyEntryRecord) -> StrategyDiffEntry {
    StrategyDiffEntry {
        key: strategy_entry_key(&record.entry),
        schema: record.entry.schema.clone(),
        grade: record.entry.grade,
        status: record.entry.status,
        script: record.entry.script.clone(),
        proof_hashes: proof_hashes(&record.entry),
        operator_attestation_hash: record
            .entry
            .operator_attestation
            .as_ref()
            .map(|attestation| attestation.attestation_hash.clone()),
        rule_id: record.entry.rule_id.clone(),
        source_file: record.source_file.clone(),
        entry_order: record.entry_order,
    }
}

fn diff_value(record: &StrategyEntryRecord) -> StrategyDiffValue {
    StrategyDiffValue {
        schema: record.entry.schema.clone(),
        grade: record.entry.grade,
        status: record.entry.status,
        script: record.entry.script.clone(),
        proof_hashes: proof_hashes(&record.entry),
        operator_attestation_hash: record
            .entry
            .operator_attestation
            .as_ref()
            .map(|attestation| attestation.attestation_hash.clone()),
        rule_id: record.entry.rule_id.clone(),
    }
}

fn classify_strategy_change(
    old: &StrategyDiffValue,
    new: &StrategyDiffValue,
) -> Vec<StrategyDiffChangeType> {
    let mut changes = Vec::new();
    if old.script.id != new.script.id {
        changes.push(StrategyDiffChangeType::ScriptIdChange);
    }
    if old.script.path != new.script.path {
        changes.push(StrategyDiffChangeType::ScriptPathChange);
    }
    if old.script.language != new.script.language {
        changes.push(StrategyDiffChangeType::ScriptLanguageChange);
    }
    if old.script.content_hash != new.script.content_hash {
        changes.push(StrategyDiffChangeType::ScriptContentHashChange);
    }
    if old.proof_hashes != new.proof_hashes {
        changes.push(StrategyDiffChangeType::ProofHashChange);
    }
    if old.schema != new.schema {
        changes.push(StrategyDiffChangeType::SchemaShapeChange);
    }
    if old.grade != new.grade {
        changes.push(StrategyDiffChangeType::GradeChange);
    }
    if old.status != new.status {
        changes.push(StrategyDiffChangeType::StatusChange);
    }
    if old.operator_attestation_hash != new.operator_attestation_hash {
        changes.push(StrategyDiffChangeType::AttestationChange);
    }
    if old.rule_id != new.rule_id {
        changes.push(StrategyDiffChangeType::RuleIdChange);
    }
    changes
}

pub(crate) fn load_schema_shape(schema_path: &Path) -> StrategyResult<StrategySchemaShape> {
    let content = fs::read_to_string(schema_path).map_err(|error| {
        Refusal::io_error(&schema_path.display().to_string(), &error.to_string())
    })?;
    let value: Value = serde_json::from_str(&content).map_err(|error| {
        crate::refusal::create_refusal(
            RefusalCode::EParse,
            format!(
                "Failed to parse schema profile '{}': {}",
                schema_path.display(),
                error
            ),
            json!({
                "schema": schema_path.display().to_string(),
                "error": error.to_string(),
            }),
            Some("Provide a JSON schema/profile artifact with columns or fields".to_string()),
        )
        .refusal
        .expect("create_refusal always sets refusal")
    })?;
    parse_schema_shape(&value).map_err(|message| {
        Refusal::strategy_input_contract(
            format!(
                "Schema profile '{}' is not a supported shape artifact: {}",
                schema_path.display(),
                message
            ),
            json!({
                "schema": schema_path.display().to_string(),
                "reason": message,
            }),
        )
    })
}

fn parse_schema_shape(value: &Value) -> Result<StrategySchemaShape, String> {
    let columns_value = value
        .get("columns")
        .or_else(|| value.get("fields"))
        .ok_or_else(|| "expected top-level 'columns' or 'fields'".to_string())?;

    let mut columns = match columns_value {
        Value::Array(values) => parse_column_array(values)?,
        Value::Object(map) => parse_column_map(map)?,
        _ => return Err("'columns' or 'fields' must be an array or object".to_string()),
    };

    if columns.is_empty() {
        return Err("schema shape must contain at least one column".to_string());
    }

    columns.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.cardinality.cmp(&right.cardinality))
    });
    let mut seen = BTreeSet::new();
    for column in &columns {
        if !seen.insert(column.name.clone()) {
            return Err(format!("duplicate column '{}'", column.name));
        }
    }
    Ok(StrategySchemaShape { columns })
}

fn parse_column_array(values: &[Value]) -> Result<Vec<StrategyColumn>, String> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| match value {
            Value::String(name) => build_column(name, "unknown", None, index),
            Value::Object(map) => {
                let name = string_field(map, &["name", "column", "field"])
                    .ok_or_else(|| format!("column entry {index} is missing a name"))?;
                let kind = string_field(map, &["type", "data_type", "kind"])
                    .unwrap_or_else(|| "unknown".to_string());
                let cardinality =
                    u64_field(map, &["cardinality", "distinct_count", "unique_count"])?;
                build_column(&name, &kind, cardinality, index)
            }
            _ => Err(format!("column entry {index} must be a string or object")),
        })
        .collect()
}

fn parse_column_map(map: &serde_json::Map<String, Value>) -> Result<Vec<StrategyColumn>, String> {
    let mut columns = Vec::new();
    for (index, (name, value)) in map.iter().enumerate() {
        match value {
            Value::String(kind) => columns.push(build_column(name, kind, None, index)?),
            Value::Object(spec) => {
                let kind = string_field(spec, &["type", "data_type", "kind"])
                    .unwrap_or_else(|| "unknown".to_string());
                let cardinality =
                    u64_field(spec, &["cardinality", "distinct_count", "unique_count"])?;
                columns.push(build_column(name, &kind, cardinality, index)?);
            }
            _ => columns.push(build_column(name, "unknown", None, index)?),
        }
    }
    Ok(columns)
}

fn build_column(
    name: &str,
    kind: &str,
    cardinality: Option<u64>,
    index: usize,
) -> Result<StrategyColumn, String> {
    let name = name.trim();
    let kind = kind.trim();
    if name.is_empty() {
        return Err(format!("column entry {index} has an empty name"));
    }
    if kind.is_empty() {
        return Err(format!("column entry {index} has an empty type"));
    }
    Ok(StrategyColumn {
        name: name.to_string(),
        kind: kind.to_string(),
        cardinality,
    })
}

fn string_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn u64_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Result<Option<u64>, String> {
    for key in keys {
        if let Some(value) = map.get(*key) {
            return match value {
                Value::Number(number) => number
                    .as_u64()
                    .map(Some)
                    .ok_or_else(|| format!("'{key}' must be a non-negative integer")),
                Value::String(raw) if raw.trim().is_empty() => Ok(None),
                Value::String(raw) => raw
                    .trim()
                    .parse::<u64>()
                    .map(Some)
                    .map_err(|error| format!("'{key}' must be a non-negative integer: {error}")),
                Value::Null => Ok(None),
                _ => Err(format!("'{key}' must be a non-negative integer")),
            };
        }
    }
    Ok(None)
}

fn candidate_for(
    record: &StrategyEntryRecord,
    query_schema: &StrategySchemaShape,
) -> Option<StrategyCandidate> {
    let schema = record.entry.schema.as_ref()?;
    let (tier, diagnostics) = compare_schema_shapes(schema, query_schema)?;
    Some(StrategyCandidate {
        tier,
        key: record.entry.key.clone(),
        schema_fingerprint: record.entry.schema_fingerprint().map(ToString::to_string),
        grade: record.entry.grade,
        status: record.entry.status,
        source_file: record.source_file.clone(),
        entry_order: record.entry_order,
        rule_id: record.entry.rule_id.clone(),
        script: record.entry.script.clone(),
        diagnostics: Some(diagnostics),
    })
}

fn task_candidate(record: &StrategyEntryRecord) -> StrategyCandidate {
    StrategyCandidate {
        tier: StrategyMatchTier::Exact,
        key: record.entry.key.clone(),
        schema_fingerprint: None,
        grade: record.entry.grade,
        status: record.entry.status,
        source_file: record.source_file.clone(),
        entry_order: record.entry_order,
        rule_id: record.entry.rule_id.clone(),
        script: record.entry.script.clone(),
        diagnostics: None,
    }
}

fn compare_schema_shapes(
    registered: &StrategySchemaShape,
    query: &StrategySchemaShape,
) -> Option<(StrategyMatchTier, StrategyMatchDiagnostics)> {
    let registered_by_name = registered
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect::<BTreeMap<_, _>>();
    let query_by_name = query
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect::<BTreeMap<_, _>>();

    let mut shared_columns = Vec::new();
    let mut missing_from_query = Vec::new();
    let mut extra_in_query = Vec::new();
    let mut type_mismatches = Vec::new();
    let mut cardinality_differences = Vec::new();

    for (name, registered_column) in &registered_by_name {
        match query_by_name.get(name) {
            Some(query_column) => {
                shared_columns.push((*name).to_string());
                if registered_column.kind != query_column.kind {
                    type_mismatches.push(StrategyTypeMismatch {
                        column: (*name).to_string(),
                        expected: registered_column.kind.clone(),
                        actual: query_column.kind.clone(),
                    });
                }
                if registered_column.cardinality != query_column.cardinality {
                    cardinality_differences.push(StrategyCardinalityDifference {
                        column: (*name).to_string(),
                        registered: registered_column.cardinality,
                        query: query_column.cardinality,
                    });
                }
            }
            None => missing_from_query.push((*name).to_string()),
        }
    }

    for name in query_by_name.keys() {
        if !registered_by_name.contains_key(name) {
            extra_in_query.push((*name).to_string());
        }
    }

    if shared_columns.is_empty() {
        return None;
    }

    let tier =
        if missing_from_query.is_empty() && extra_in_query.is_empty() && type_mismatches.is_empty()
        {
            if cardinality_differences.is_empty() {
                StrategyMatchTier::Exact
            } else {
                StrategyMatchTier::Compatible
            }
        } else {
            StrategyMatchTier::Partial
        };

    Some((
        tier,
        StrategyMatchDiagnostics {
            shared_columns,
            missing_from_query,
            extra_in_query,
            type_mismatches,
            cardinality_differences,
        },
    ))
}

fn resolve_skill_hash(
    skill_path: Option<&Path>,
    skill_hash: Option<&str>,
) -> StrategyResult<String> {
    match (skill_path, skill_hash) {
        (Some(path), None) => hash_file(path),
        (None, Some(hash)) => {
            let hash = hash.trim();
            if hash.is_empty() {
                Err(Refusal::strategy_input_contract(
                    "--skill-hash cannot be empty",
                    json!({ "skill_hash": hash }),
                ))
            } else {
                Ok(hash.to_string())
            }
        }
        _ => Err(Refusal::strategy_input_contract(
            "Exactly one of --skill or --skill-hash is required",
            json!({
                "has_skill": skill_path.is_some(),
                "has_skill_hash": skill_hash.is_some(),
            }),
        )),
    }
}

pub(crate) fn fingerprint_schema(schema: &StrategySchemaShape) -> StrategyResult<String> {
    let bytes = serde_json::to_vec(schema).map_err(|error| {
        Refusal::strategy_input_contract(
            "Failed to serialize schema shape for fingerprinting",
            json!({ "error": error.to_string() }),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn hash_file(path: &Path) -> StrategyResult<String> {
    let bytes = fs::read(path)
        .map_err(|error| Refusal::io_error(&path.display().to_string(), &error.to_string()))?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn hash_registry_state(registry_dir: &Path) -> StrategyResult<String> {
    let mut bytes = Vec::new();
    let registry_json = registry_dir.join("registry.json");
    append_hash_file(&mut bytes, "registry.json", &registry_json)?;
    let strategy_dir = registry_dir.join(STRATEGY_DIR);
    if strategy_dir.exists() {
        let mut paths = fs::read_dir(&strategy_dir)
            .map_err(|error| {
                Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("failed to read _strategy directory: {error}"),
                )
            })?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                Refusal::bad_registry(
                    &registry_dir.display().to_string(),
                    &format!("failed to read _strategy entry: {error}"),
                )
            })?;
        paths.retain(|path| path.is_file() && path.extension() == Some("json".as_ref()));
        paths.sort();
        for path in paths {
            let label = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("{STRATEGY_DIR}/{name}"))
                .unwrap_or_else(|| path.display().to_string());
            append_hash_file(&mut bytes, &label, &path)?;
        }
    }
    Ok(hash_bytes(&bytes))
}

fn append_hash_file(buffer: &mut Vec<u8>, label: &str, path: &Path) -> StrategyResult<()> {
    buffer.extend(label.as_bytes());
    buffer.push(0);
    buffer.extend(fs::read(path).map_err(|error| {
        Refusal::bad_registry(
            &path.display().to_string(),
            &format!("failed to read registry hash input: {error}"),
        )
    })?);
    buffer.push(0);
    Ok(())
}

fn required_non_empty<'a>(label: &str, value: &'a str) -> StrategyResult<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        Err(Refusal::strategy_input_contract(
            format!("{label} cannot be empty"),
            json!({ "field": label }),
        ))
    } else {
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy)]
enum ProofKind {
    Verify,
    Assess,
    Airlock,
}

impl ProofKind {
    fn label(self) -> &'static str {
        match self {
            Self::Verify => "verify",
            Self::Assess => "assess",
            Self::Airlock => "airlock",
        }
    }
}

fn load_proof_reference(path: &Path, kind: ProofKind) -> StrategyResult<StrategyProofReference> {
    let bytes = fs::read(path)
        .map_err(|error| Refusal::io_error(&path.display().to_string(), &error.to_string()))?;
    let content_hash = hash_bytes(&bytes);
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        Refusal::strategy_proof_invalid(
            format!("{} proof must be valid JSON", kind.label()),
            json!({
                "proof": kind.label(),
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;
    let decision = accepted_proof_decision(&value, kind).ok_or_else(|| {
        Refusal::strategy_proof_invalid(
            format!(
                "{} proof did not contain an accepted pass decision",
                kind.label()
            ),
            json!({
                "proof": kind.label(),
                "path": path.display().to_string(),
                "accepted": accepted_decisions(kind),
            }),
        )
    })?;
    Ok(StrategyProofReference {
        path: path.display().to_string(),
        content_hash,
        decision,
    })
}

fn accepted_proof_decision(value: &Value, kind: ProofKind) -> Option<String> {
    match kind {
        ProofKind::Verify => {
            if value.get("passed").and_then(Value::as_bool) == Some(true) {
                return Some("passed:true".to_string());
            }
        }
        ProofKind::Airlock => {
            if value.get("sealed").and_then(Value::as_bool) == Some(true) {
                return Some("sealed:true".to_string());
            }
        }
        ProofKind::Assess => {}
    }

    let keys = match kind {
        ProofKind::Assess => ["decision", "status", "outcome", "result"],
        ProofKind::Verify | ProofKind::Airlock => ["status", "outcome", "result", "decision"],
    };
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .find_map(|raw| {
            let normalized = raw.trim().to_ascii_uppercase();
            if accepted_decisions(kind).contains(&normalized.as_str()) {
                Some(normalized)
            } else {
                None
            }
        })
}

fn accepted_decisions(kind: ProofKind) -> &'static [&'static str] {
    match kind {
        ProofKind::Verify => &["PASS", "PASSED", "SUCCESS"],
        ProofKind::Assess => &["PROCEED"],
        ProofKind::Airlock => &["PASS", "PASSED", "SEALED", "SUCCESS"],
    }
}

fn append_strategy_entry(
    registry_dir: &Path,
    new_entry: &StrategyRegistryEntry,
) -> StrategyResult<usize> {
    let strategy_dir = registry_dir.join(STRATEGY_DIR);
    let entries_path = strategy_dir.join(DEFAULT_ENTRIES_FILE);
    let mut entries = if entries_path.exists() {
        let content = fs::read_to_string(&entries_path).map_err(|error| {
            Refusal::bad_registry(
                &registry_dir.display().to_string(),
                &format!(
                    "failed to read strategy entries file '{}': {error}",
                    entries_path.display()
                ),
            )
        })?;
        serde_json::from_str::<Vec<StrategyRegistryEntry>>(&content).map_err(|error| {
            Refusal::bad_registry(
                &registry_dir.display().to_string(),
                &format!(
                    "failed to parse strategy entries file '{}': {error}",
                    entries_path.display()
                ),
            )
        })?
    } else {
        Vec::new()
    };
    let entry_order = entries.len();
    entries.push(new_entry.clone());

    fs::create_dir_all(&strategy_dir).map_err(|error| {
        write_refusal(&strategy_dir, "Failed to create _strategy directory", error)
    })?;
    let content = format!(
        "{}\n",
        serde_json::to_string_pretty(&entries).map_err(|error| {
            Refusal::strategy_input_contract(
                "Failed to serialize strategy registry entries",
                json!({ "error": error.to_string() }),
            )
        })?
    );
    fs::write(&entries_path, content).map_err(|error| {
        write_refusal(
            &entries_path,
            "Failed to write strategy entries file",
            error,
        )
    })?;
    Ok(entry_order)
}

fn rewrite_strategy_entry(
    registry_dir: &Path,
    source_file: &str,
    entry_order: usize,
    replacement: &StrategyRegistryEntry,
) -> StrategyResult<usize> {
    let entries_path = registry_dir.join(STRATEGY_DIR).join(source_file);
    let content = fs::read_to_string(&entries_path).map_err(|error| {
        Refusal::bad_registry(
            &registry_dir.display().to_string(),
            &format!(
                "failed to read strategy entries file '{}': {error}",
                entries_path.display()
            ),
        )
    })?;
    let mut entries =
        serde_json::from_str::<Vec<StrategyRegistryEntry>>(&content).map_err(|error| {
            Refusal::bad_registry(
                &registry_dir.display().to_string(),
                &format!(
                    "failed to parse strategy entries file '{}': {error}",
                    entries_path.display()
                ),
            )
        })?;
    let slot = entries.get_mut(entry_order).ok_or_else(|| {
        Refusal::bad_registry(
            &registry_dir.display().to_string(),
            &format!(
                "strategy entry order {} no longer exists in '{}'",
                entry_order,
                entries_path.display()
            ),
        )
    })?;
    *slot = replacement.clone();
    let content = format!(
        "{}\n",
        serde_json::to_string_pretty(&entries).map_err(|error| {
            Refusal::strategy_input_contract(
                "Failed to serialize strategy registry entries",
                json!({ "error": error.to_string() }),
            )
        })?
    );
    fs::write(&entries_path, content).map_err(|error| {
        write_refusal(
            &entries_path,
            "Failed to write strategy entries file",
            error,
        )
    })?;
    Ok(entry_order)
}

fn entry_to_registered(entry: &StrategyRegistryEntry) -> StrategyRegisteredEntry {
    StrategyRegisteredEntry {
        key: entry.key.clone(),
        schema_fingerprint: entry.schema_fingerprint().map(ToString::to_string),
        task: entry.task().map(ToString::to_string),
        skill_hash: entry.key.skill_hash().to_string(),
        grade: entry.grade,
        status: entry.status,
        script: entry.script.clone(),
        rule_id: entry.rule_id.clone(),
    }
}

fn entry_to_catalog(record: &StrategyEntryRecord) -> StrategyCatalogEntry {
    StrategyCatalogEntry {
        key: record.entry.key.clone(),
        schema_fingerprint: record.entry.schema_fingerprint().map(ToString::to_string),
        task: record.entry.task().map(ToString::to_string),
        skill_hash: record.entry.key.skill_hash().to_string(),
        grade: record.entry.grade,
        status: record.entry.status,
        script: record.entry.script.clone(),
        rule_id: record.entry.rule_id.clone(),
        source_file: record.source_file.clone(),
        entry_order: record.entry_order,
        operator: record
            .entry
            .operator_attestation
            .as_ref()
            .map(|attestation| attestation.operator.clone()),
        reason: record
            .entry
            .operator_attestation
            .as_ref()
            .map(|attestation| attestation.reason.clone())
            .or_else(|| {
                record
                    .entry
                    .deprecation
                    .as_ref()
                    .map(|deprecation| deprecation.reason.clone())
            }),
    }
}

fn update_registry_json(
    registry_dir: &Path,
    mut registry_json: RegistryJson,
    next_version: &str,
    entry_count: usize,
) -> StrategyResult<()> {
    registry_json.version = next_version.to_string();
    registry_json.entry_count = entry_count;
    let path = registry_dir.join("registry.json");
    let content = format!(
        "{}\n",
        serde_json::to_string_pretty(&registry_json).map_err(|error| {
            Refusal::bad_registry(
                &registry_dir.display().to_string(),
                &format!("failed to serialize registry.json: {error}"),
            )
        })?
    );
    fs::write(&path, content).map_err(|error| {
        write_refusal(
            &path,
            "Failed to write updated strategy registry metadata",
            error,
        )
    })
}

fn write_refusal(path: &Path, message: &str, error: std::io::Error) -> Refusal {
    Refusal {
        code: RefusalCode::EIo,
        message: format!("{message} '{}': {error}", path.display()),
        detail: json!({
            "path": path.display().to_string(),
            "error": error.to_string(),
        }),
        next_command: Some("Check paths and permissions, then rerun canon strategy".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn schema(columns: &[(&str, &str, Option<u64>)]) -> StrategySchemaShape {
        let mut columns = columns
            .iter()
            .map(|(name, kind, cardinality)| StrategyColumn {
                name: (*name).to_string(),
                kind: (*kind).to_string(),
                cardinality: *cardinality,
            })
            .collect::<Vec<_>>();
        columns.sort_by(|left, right| left.name.cmp(&right.name));
        StrategySchemaShape { columns }
    }

    fn registry_entry(
        shape: StrategySchemaShape,
        skill_hash: &str,
        script_id: &str,
        script_hash: &str,
    ) -> StrategyRegistryEntry {
        let schema_fingerprint = fingerprint_schema(&shape).unwrap();
        StrategyRegistryEntry {
            entry_schema_version: STRATEGY_ENTRY_SCHEMA_VERSION.to_string(),
            key: StrategyEntryKey::Schema {
                schema_fingerprint,
                skill_hash: skill_hash.to_string(),
            },
            schema: Some(shape),
            grade: StrategyAttestationGrade::ProofAttested,
            status: StrategyEntryStatus::Active,
            skill_hash: skill_hash.to_string(),
            script: FrozenScript {
                id: script_id.to_string(),
                path: format!("scripts/{script_id}.py"),
                language: "python".to_string(),
                content_hash: script_hash.to_string(),
            },
            proofs: Some(StrategyProofs {
                verify: proof("verify", "blake3:verify"),
                assess: proof("assess", "blake3:assess"),
                airlock: proof("airlock", "blake3:airlock"),
            }),
            operator_attestation: None,
            deprecation: None,
            rule_id: "STRATEGY_CHAMPION".to_string(),
        }
    }

    fn proof(label: &str, hash: &str) -> StrategyProofReference {
        StrategyProofReference {
            path: format!("evidence/{label}.json"),
            content_hash: hash.to_string(),
            decision: "PASS".to_string(),
        }
    }

    fn write_strategy_registry(
        path: &Path,
        id: &str,
        version: &str,
        entries: &[StrategyRegistryEntry],
    ) {
        fs::write(
            path.join("registry.json"),
            serde_json::to_string_pretty(&json!({
                "id": id,
                "version": version,
                "description": "test strategy registry",
                "updated": "2026-01-01",
                "entry_count": entries.len()
            }))
            .unwrap(),
        )
        .unwrap();
        let strategy_dir = path.join(STRATEGY_DIR);
        fs::create_dir_all(&strategy_dir).unwrap();
        fs::write(
            strategy_dir.join(DEFAULT_ENTRIES_FILE),
            serde_json::to_string_pretty(entries).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn parses_profile_columns_and_hashes_canonically() {
        let value = json!({
            "columns": [
                {"name": "amount", "type": "number", "distinct_count": 10},
                {"name": "vendor", "type": "string", "cardinality": 3}
            ]
        });
        let shape = parse_schema_shape(&value).unwrap();
        assert_eq!(shape.columns[0].name, "amount");
        assert_eq!(shape.columns[1].name, "vendor");
        assert!(fingerprint_schema(&shape).unwrap().starts_with("blake3:"));
    }

    #[test]
    fn compares_exact_compatible_and_partial_shapes() {
        let registered = schema(&[
            ("amount", "number", Some(10)),
            ("vendor", "string", Some(3)),
        ]);
        let exact = schema(&[
            ("vendor", "string", Some(3)),
            ("amount", "number", Some(10)),
        ]);
        let compatible = schema(&[
            ("amount", "number", Some(99)),
            ("vendor", "string", Some(42)),
        ]);
        let partial = schema(&[
            ("amount", "number", Some(10)),
            ("category", "string", Some(5)),
        ]);

        assert_eq!(
            compare_schema_shapes(&registered, &exact).unwrap().0,
            StrategyMatchTier::Exact
        );
        assert_eq!(
            compare_schema_shapes(&registered, &compatible).unwrap().0,
            StrategyMatchTier::Compatible
        );
        assert_eq!(
            compare_schema_shapes(&registered, &partial).unwrap().0,
            StrategyMatchTier::Partial
        );
    }

    #[test]
    fn proof_gates_require_expected_decisions() {
        assert_eq!(
            accepted_proof_decision(&json!({"status":"PASS"}), ProofKind::Verify),
            Some("PASS".to_string())
        );
        assert_eq!(
            accepted_proof_decision(&json!({"decision":"PROCEED"}), ProofKind::Assess),
            Some("PROCEED".to_string())
        );
        assert_eq!(
            accepted_proof_decision(&json!({"sealed":true}), ProofKind::Airlock),
            Some("sealed:true".to_string())
        );
        assert_eq!(
            accepted_proof_decision(&json!({"decision":"RETRY"}), ProofKind::Assess),
            None
        );
    }

    #[test]
    fn classifies_strategy_change_types_independently() {
        let old_shape = schema(&[("amount", "number", Some(10))]);
        let new_shape = schema(&[("amount", "number", Some(11))]);
        let old = StrategyDiffValue {
            schema: Some(old_shape),
            grade: StrategyAttestationGrade::ProofAttested,
            status: StrategyEntryStatus::Active,
            script: FrozenScript {
                id: "script-a".to_string(),
                path: "scripts/a.py".to_string(),
                language: "python".to_string(),
                content_hash: "blake3:old".to_string(),
            },
            proof_hashes: StrategyProofHashes {
                verify: "blake3:verify-old".to_string(),
                assess: "blake3:assess-old".to_string(),
                airlock: "blake3:airlock-old".to_string(),
            },
            operator_attestation_hash: None,
            rule_id: "OLD_RULE".to_string(),
        };
        let new = StrategyDiffValue {
            schema: Some(new_shape),
            grade: StrategyAttestationGrade::ProofAttested,
            status: StrategyEntryStatus::Active,
            script: FrozenScript {
                id: "script-b".to_string(),
                path: "scripts/b.py".to_string(),
                language: "bash".to_string(),
                content_hash: "blake3:new".to_string(),
            },
            proof_hashes: StrategyProofHashes {
                verify: "blake3:verify-new".to_string(),
                assess: "blake3:assess-new".to_string(),
                airlock: "blake3:airlock-new".to_string(),
            },
            operator_attestation_hash: None,
            rule_id: "NEW_RULE".to_string(),
        };

        assert_eq!(
            classify_strategy_change(&old, &new),
            vec![
                StrategyDiffChangeType::ScriptIdChange,
                StrategyDiffChangeType::ScriptPathChange,
                StrategyDiffChangeType::ScriptLanguageChange,
                StrategyDiffChangeType::ScriptContentHashChange,
                StrategyDiffChangeType::ProofHashChange,
                StrategyDiffChangeType::SchemaShapeChange,
                StrategyDiffChangeType::RuleIdChange,
            ]
        );
    }

    #[test]
    fn diff_reports_add_remove_change_and_unchanged_entries() {
        let old_dir = tempdir().unwrap();
        let new_dir = tempdir().unwrap();
        let unchanged = registry_entry(
            schema(&[("vendor", "string", Some(3))]),
            "blake3:skill-a",
            "unchanged",
            "blake3:unchanged",
        );
        let removed = registry_entry(
            schema(&[("removed", "string", Some(1))]),
            "blake3:skill-b",
            "removed",
            "blake3:removed",
        );
        let mut changed_old = registry_entry(
            schema(&[("changed", "number", Some(2))]),
            "blake3:skill-c",
            "changed",
            "blake3:old-script",
        );
        changed_old.rule_id = "OLD_RULE".to_string();
        let mut changed_new = changed_old.clone();
        changed_new.script.content_hash = "blake3:new-script".to_string();
        changed_new.proofs.as_mut().unwrap().verify.content_hash = "blake3:new-verify".to_string();
        changed_new.rule_id = "NEW_RULE".to_string();
        let added = registry_entry(
            schema(&[("added", "string", Some(4))]),
            "blake3:skill-d",
            "added",
            "blake3:added",
        );

        write_strategy_registry(
            old_dir.path(),
            "strategy-test",
            "0.1.0",
            &[unchanged.clone(), removed, changed_old],
        );
        write_strategy_registry(
            new_dir.path(),
            "strategy-test",
            "0.2.0",
            &[unchanged.clone(), changed_new, added],
        );

        let output = diff(old_dir.path(), new_dir.path()).unwrap();
        assert_eq!(output.version, "canon_strategy_diff.v0");
        assert_eq!(output.summary.total_old, 3);
        assert_eq!(output.summary.total_new, 3);
        assert_eq!(output.summary.added, 1);
        assert_eq!(output.summary.removed, 1);
        assert_eq!(output.summary.changed, 1);
        assert_eq!(output.summary.unchanged, 1);
        assert_eq!(output.unchanged[0].key, strategy_entry_key(&unchanged));
        assert_eq!(
            output.changed[0].change_types,
            vec![
                StrategyDiffChangeType::ScriptContentHashChange,
                StrategyDiffChangeType::ProofHashChange,
                StrategyDiffChangeType::RuleIdChange,
            ]
        );
    }

    #[test]
    fn diff_refuses_mismatched_registry_ids() {
        let old_dir = tempdir().unwrap();
        let new_dir = tempdir().unwrap();
        write_strategy_registry(old_dir.path(), "old-id", "0.1.0", &[]);
        write_strategy_registry(new_dir.path(), "new-id", "0.2.0", &[]);

        let refusal = diff(old_dir.path(), new_dir.path()).unwrap_err();
        assert_eq!(refusal.code, RefusalCode::EBadRegistry);
        assert!(refusal.message.contains("new-id"));
    }

    #[test]
    fn diff_uses_first_entry_for_duplicate_strategy_keys() {
        let old_dir = tempdir().unwrap();
        let new_dir = tempdir().unwrap();
        let primary = registry_entry(
            schema(&[("vendor", "string", Some(3))]),
            "blake3:skill",
            "primary",
            "blake3:primary",
        );
        let mut shadowed = primary.clone();
        shadowed.script.id = "shadowed".to_string();
        shadowed.script.content_hash = "blake3:shadowed".to_string();

        write_strategy_registry(
            old_dir.path(),
            "strategy-test",
            "0.1.0",
            &[primary.clone(), shadowed],
        );
        write_strategy_registry(new_dir.path(), "strategy-test", "0.2.0", &[primary]);

        let output = diff(old_dir.path(), new_dir.path()).unwrap();
        assert_eq!(output.summary.total_old, 1);
        assert_eq!(output.summary.total_new, 1);
        assert_eq!(output.summary.changed, 0);
        assert_eq!(output.summary.unchanged, 1);
        assert_eq!(output.unchanged[0].script.id, "primary");
    }

    #[test]
    fn register_then_resolve_exact_strategy() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("registry.json"),
            serde_json::to_string_pretty(&json!({
                "id": "strategy-test",
                "version": "0.1.0",
                "description": "test",
                "updated": "2026-01-01",
                "entry_count": 0
            }))
            .unwrap(),
        )
        .unwrap();
        let schema_path = dir.path().join("schema.json");
        fs::write(
            &schema_path,
            serde_json::to_string_pretty(&json!({
                "columns": [
                    {"name": "vendor", "type": "string", "cardinality": 3},
                    {"name": "amount", "type": "number", "cardinality": 10}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let skill_path = dir.path().join("SKILL.md");
        let script_path = dir.path().join("script.py");
        let verify_path = dir.path().join("verify.json");
        let assess_path = dir.path().join("assess.json");
        let airlock_path = dir.path().join("airlock.json");
        fs::write(&skill_path, "skill body").unwrap();
        fs::write(&script_path, "print('ok')").unwrap();
        fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
        fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
        fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

        let registered = register(StrategyRegisterRequest {
            registry_dir: dir.path(),
            key: StrategyKeySelector::Schema(&schema_path),
            skill_path: Some(&skill_path),
            skill_hash: None,
            script_path: &script_path,
            script_id: "procurement-total.v1",
            language: "python",
            grade: StrategyAttestationGrade::ProofAttested,
            operator: None,
            reason: None,
            attested_at: None,
            verify_path: Some(&verify_path),
            assess_path: Some(&assess_path),
            airlock_path: Some(&airlock_path),
            next_version: "0.2.0",
            rule_id: None,
        })
        .unwrap();
        assert_eq!(registered.registry.version, "0.2.0");
        assert_eq!(registered.entry_count, 1);

        let resolved = resolve(StrategyResolveRequest {
            registry_dir: dir.path(),
            key: StrategyKeySelector::Schema(&schema_path),
            skill_path: Some(&skill_path),
            skill_hash: None,
        })
        .unwrap();
        assert_eq!(resolved.outcome, StrategyResolveOutcome::Exact);
        assert_eq!(resolved.exit_code(), 0);
        assert_eq!(
            resolved.resolved_match.unwrap().script.id,
            "procurement-total.v1"
        );
    }
}
