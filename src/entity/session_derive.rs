#![forbid(unsafe_code)]

//! `canon entity session derive`: Canon's existing entity operators run over
//! the session's observations and append what they find to the session as
//! canon.evidence.v1 records, through the same assert path an agent uses.
//!
//! Nothing here reimplements blocking or scoring. The observations become a
//! rows file, the existing prepare -> index -> block -> evidence stages run on
//! it with the caller's profile and strategy, and every support or
//! cannot-link hit becomes one assertion line. Similarity is ranking material,
//! not proof: it is stored as `name_similar` or `value_similar`, which solve
//! never counts toward tier A or B.

use crate::{
    Refusal, RefusalCode,
    entity::{
        block_artifact::ExactBucketAssertion,
        edge::EdgeEvidenceRecord,
        prepare::{PreparedSurfaceRecord, load_prepare_profile_with_hash},
        publication::open_current_stream_generation,
        run::{EntityRunRequest, run_entity_workbench, unwired_run_operators},
        score::ScoreLane,
        session_closure_export::shard_lines,
        session_explain::{explain_refusal, read_manifest},
        session_names::{
            FragmentRejection, NameNode, NameStageStats, classify_observation, effective_type,
            name_assertions, person_views,
        },
        session_slots::{SAME_SLOT_OPERATOR, SlotStats, slot_assertions, slot_rules},
        session_structure::{
            StructureNode, StructureStats, multi_party_ids, structure_assertions, structure_views,
        },
    },
    session::{SESSION_OBSERVATIONS_STREAM_ID, SessionAssertRequest, assert_evidence},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const SESSION_DERIVE_VERSION: &str = "canon.entity.session.derive.v1";
const DERIVE_WORK_DIR: &str = "derive-work";

#[derive(Debug, Clone)]
pub struct SessionDeriveRequest {
    pub session: PathBuf,
    pub profile: String,
    pub strategy: PathBuf,
    /// Registry aliases are candidates for the session's surfaces (day two).
    /// `false` is `--no-registry-candidates`.
    pub registry_candidates: bool,
}

/// What the registry contributed, measured from the block stage's own
/// candidate file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RegistryCandidateStats {
    pub enabled: bool,
    /// Registry alias rows added to the rows file.
    pub rows: usize,
    /// Candidate pairs between a session surface and a registry row.
    pub session_registry_pairs: usize,
    /// Candidate pairs between two registry rows; the registry's size must not
    /// make this grow, and block never generates one.
    pub registry_registry_pairs_scored: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionDeriveRejection {
    pub obs_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionDeriveOutput {
    pub version: String,
    pub profile: String,
    pub profile_hash: String,
    pub observations: usize,
    pub surfaces: usize,
    /// Records the operators produced, by evidence type and by operator.
    pub records_by_type: BTreeMap<String, usize>,
    pub records_by_operator: BTreeMap<String, usize>,
    pub accepted: usize,
    pub already_present: usize,
    pub rejected_assertions: usize,
    /// Observations the prepare stage could not normalise, by obs_id.
    pub rejected_observations: Vec<SessionDeriveRejection>,
    /// Hits between observations of different entity types, never recorded.
    pub cross_type_skipped: usize,
    /// Person and trust names (a profile view with the `person_name` operator):
    /// observations by classified type, fragments rejected, people compared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names: Option<NameStageStats>,
    /// Alternates (dba / fka) and member lists (a profile view with the
    /// `name_structure` operator): surfaces parsed, alternates by marker,
    /// multi_party surfaces and the records they produced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structure: Option<StructureStats>,
    /// Shared slots, when the profile declares `same_slot`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<SlotStats>,
    /// Exact buckets stored as surface ranges, which derive does not expand.
    pub exact_bucket_ranges_skipped: usize,
    /// Declared relation hints: a profile must list them, `entity run` emits one
    /// generic review hint instead, and derive runs none of them.
    pub relation_hints_not_executed: Vec<String>,
    pub registry_candidates: RegistryCandidateStats,
    /// Hash of the sorted assertion lines this run produced: the same cut gives
    /// the same digest whatever else the registry holds.
    pub records_digest: String,
    pub shard: Option<String>,
    pub generation_id: Option<String>,
}

impl SessionDeriveOutput {
    pub fn render_summary(&self) -> String {
        let by_type = self
            .records_by_type
            .iter()
            .map(|(kind, count)| format!("{kind}={count}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "observations={} surfaces={} records={} accepted={} already_present={} rejected_observations={} cross_type_skipped={} registry_rows={} registry_registry_pairs_scored={} generation={}",
            self.observations,
            self.surfaces,
            by_type,
            self.accepted,
            self.already_present,
            self.rejected_observations.len(),
            self.cross_type_skipped,
            self.registry_candidates.rows,
            self.registry_candidates.registry_registry_pairs_scored,
            self.generation_id.as_deref().unwrap_or("<none>"),
        )
    }
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path, what: &str) -> Result<Vec<T>, Refusal> {
    let text = fs::read_to_string(path).map_err(|error| {
        explain_refusal(
            format!(
                "Unable to read the derive {what} {}: {error}",
                path.display()
            ),
            json!({"writes_performed": false}),
            "Rerun canon entity session derive",
        )
    })?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<T>(line).map_err(|error| {
                explain_refusal(
                    format!("The derive {what} is invalid: {error}"),
                    json!({"writes_performed": false}),
                    "Rerun canon entity session derive",
                )
            })
        })
        .collect()
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// The rows file the existing stages run on: one row per observation, with
/// `obs_id`, `source_row_id` (the same id), `surface`, `entity_type`, each key
/// namespace as a column, each attribute as a column (`attr_<name>` when it
/// would collide) and each context field as `context_<name>`.
fn write_rows(
    path: &Path,
    observations: &[Value],
    registry_rows: &[RegistryRow],
) -> Result<Vec<String>, Refusal> {
    let mut key_columns = BTreeSet::new();
    for row in registry_rows {
        key_columns.extend(row.keys.keys().cloned());
    }
    let mut attr_columns = BTreeSet::new();
    let mut context_columns = BTreeSet::new();
    for observation in observations {
        for (field, columns) in [
            ("keys", &mut key_columns),
            ("attrs", &mut attr_columns),
            ("context", &mut context_columns),
        ] {
            if let Some(object) = observation.get(field).and_then(Value::as_object) {
                columns.extend(
                    object
                        .iter()
                        .filter(|(_, value)| text_of(value).is_some())
                        .map(|(name, _)| name.clone()),
                );
            }
        }
    }
    let reserved = [
        "obs_id",
        "source_row_id",
        "surface",
        "entity_type",
        "origin",
    ];
    let attr_header = |name: &str| {
        if reserved.contains(&name) || key_columns.contains(name) {
            format!("attr_{name}")
        } else {
            name.to_string()
        }
    };
    let mut header = reserved
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    header.extend(key_columns.iter().cloned());
    header.extend(attr_columns.iter().map(|name| attr_header(name)));
    header.extend(context_columns.iter().map(|name| format!("context_{name}")));

    let io_error = |error: String| {
        explain_refusal(
            format!("Unable to write the derive rows file: {error}"),
            json!({"path": path.display().to_string(), "writes_performed": true}),
            "Rerun canon entity session derive",
        )
    };
    let mut writer = csv::Writer::from_path(path).map_err(|error| io_error(error.to_string()))?;
    writer
        .write_record(&header)
        .map_err(|error| io_error(error.to_string()))?;
    let mut order = Vec::new();
    for observation in observations {
        let obs_id = observation
            .get("obs_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let field_text = |object: &str, name: &str| {
            observation
                .get(object)
                .and_then(|value| value.get(name))
                .and_then(text_of)
                .unwrap_or_default()
        };
        let mut row = vec![
            obs_id.to_string(),
            obs_id.to_string(),
            observation
                .get("surface")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            observation
                .get("entity_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            "session".to_string(),
        ];
        row.extend(key_columns.iter().map(|name| field_text("keys", name)));
        row.extend(attr_columns.iter().map(|name| field_text("attrs", name)));
        row.extend(
            context_columns
                .iter()
                .map(|name| field_text("context", name)),
        );
        writer
            .write_record(&row)
            .map_err(|error| io_error(error.to_string()))?;
        order.push(obs_id.to_string());
    }
    // Registry rows: one per alias, keyed `registry:alias:<input>`, carrying the
    // key aliases of its id. Marked `origin: registry` so block never pairs two.
    for registry_row in registry_rows {
        let mut row = vec![
            registry_row.row_id(),
            registry_row.row_id(),
            registry_row.input.clone(),
            registry_row.entity_type.clone(),
            "registry".to_string(),
        ];
        row.extend(
            key_columns
                .iter()
                .map(|name| registry_row.keys.get(name).cloned().unwrap_or_default()),
        );
        row.extend(attr_columns.iter().map(|_| String::new()));
        row.extend(context_columns.iter().map(|_| String::new()));
        writer
            .write_record(&row)
            .map_err(|error| io_error(error.to_string()))?;
    }
    writer
        .flush()
        .map_err(|error| io_error(error.to_string()))?;
    Ok(order)
}

/// One registry alias as a candidate row.
#[derive(Debug, Clone)]
struct RegistryRow {
    input: String,
    canonical_id: String,
    entity_type: String,
    /// The id's key aliases by namespace.
    keys: BTreeMap<String, String>,
}

impl RegistryRow {
    fn row_id(&self) -> String {
        format!("registry:alias:{}", self.input)
    }
}

/// The aliases of the registries the session maps, as candidate rows: name
/// aliases only (key aliases ride along as columns, retired and agent ids are
/// not names), first entry in file order per input, as exact lookup reads them.
fn registry_rows(
    registries: &[crate::session::SessionRegistryBinding],
) -> Result<Vec<RegistryRow>, Refusal> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for binding in registries {
        let dir = Path::new(&binding.path);
        let mut files = fs::read_dir(dir)
            .map_err(|error| {
                explain_refusal(
                    format!("Unable to read registry {}: {error}", dir.display()),
                    json!({"writes_performed": false}),
                    "Check the session's registry binding",
                )
            })?
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
        let mut entries = Vec::new();
        for file in files {
            if let Ok(bytes) = fs::read(&file)
                && let Ok(Value::Array(found)) = serde_json::from_slice::<Value>(&bytes)
            {
                entries.extend(found);
            }
        }
        let text = |entry: &Value, name: &str| {
            entry
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let mut keys_by_id = BTreeMap::<String, BTreeMap<String, String>>::new();
        for entry in &entries {
            if let Some(namespace) = text(entry, "rule_id").strip_prefix("SESSION_KEY_") {
                keys_by_id
                    .entry(text(entry, "canonical_id"))
                    .or_default()
                    .entry(namespace.to_ascii_lowercase())
                    .or_insert_with(|| text(entry, "input"));
            }
        }
        for entry in &entries {
            let rule = text(entry, "rule_id");
            if rule.starts_with("SESSION_KEY_")
                || rule == "SUPERSEDED_ID_ALIAS"
                || rule == "SESSION_AGENT_ID"
            {
                continue;
            }
            let input = text(entry, "input");
            let canonical_id = text(entry, "canonical_id");
            if input.is_empty() || canonical_id.is_empty() || !seen.insert(input.clone()) {
                continue;
            }
            rows.push(RegistryRow {
                keys: keys_by_id.get(&canonical_id).cloned().unwrap_or_default(),
                input,
                canonical_id,
                entity_type: binding.kind.clone(),
            });
        }
    }
    Ok(rows)
}

/// What an operator hit means as session evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DerivedKind {
    /// `exact_view` on a normalised view.
    NameEquivalent,
    /// `anchor_match`, `isin_cusip_arithmetic`.
    SameKey,
    /// `anchor_conflict`, `attribute_conflict`, `protected_token_conflict`.
    Distinct,
    /// `string_similarity`, `tfidf_cosine`, `two_token_reversal`, `token_overlap`.
    NameSimilar,
    /// `date_transposed_digits`.
    ValueSimilar,
    /// `numeric_within_tolerance`, `date_exact`, `categorical_match`: one record
    /// per pair listing every agreeing attribute, so three agreeing attributes
    /// are one evidence type, not three.
    AttributeAgreement,
}

impl DerivedKind {
    pub(crate) fn evidence_type(self) -> &'static str {
        match self {
            Self::NameEquivalent => "name_equivalent",
            Self::SameKey => "same_key",
            Self::Distinct => "distinct",
            Self::NameSimilar => "name_similar",
            Self::ValueSimilar => "value_similar",
            Self::AttributeAgreement => "attribute_agreement",
        }
    }
}

/// The base operator of an operator id such as `exact_view:core`.
fn base_operator(operator_id: &str) -> &str {
    operator_id.split(':').next().unwrap_or(operator_id)
}

/// Mapping of one hit to a session evidence kind; `None` for hits that are not
/// evidence about two surfaces (relation hints).
pub(crate) fn kind_for_hit(lane: ScoreLane, operator_id: &str) -> Option<DerivedKind> {
    match (lane, base_operator(operator_id)) {
        (ScoreLane::Support, "exact_view") => Some(DerivedKind::NameEquivalent),
        (ScoreLane::Support, "anchor_match" | "isin_cusip_arithmetic") => {
            Some(DerivedKind::SameKey)
        }
        (
            ScoreLane::Support,
            "string_similarity" | "tfidf_cosine" | "two_token_reversal" | "token_overlap",
        ) => Some(DerivedKind::NameSimilar),
        (ScoreLane::Support, "date_transposed_digits") => Some(DerivedKind::ValueSimilar),
        (ScoreLane::Support, "numeric_within_tolerance" | "date_exact" | "categorical_match") => {
            Some(DerivedKind::AttributeAgreement)
        }
        (
            ScoreLane::AntiMerge,
            "anchor_conflict" | "attribute_conflict" | "protected_token_conflict",
        ) => Some(DerivedKind::Distinct),
        _ => None,
    }
}

struct SurfaceInfo<'a> {
    obs_id: String,
    entity_type: String,
    record: &'a PreparedSurfaceRecord,
    /// For a registry alias row: the id it names.
    canonical_id: Option<String>,
}

impl SurfaceInfo<'_> {
    fn is_registry(&self) -> bool {
        self.obs_id.starts_with("registry:alias:")
    }

    /// The id the evidence record names: an observation id, or the registry
    /// endpoint form (`registry:alias:<input>`; `registry:<id>` for a shared
    /// key, which is evidence about the incumbent itself).
    fn target(&self, kind: DerivedKind) -> String {
        match (&self.canonical_id, kind) {
            (Some(id), DerivedKind::SameKey) => format!("registry:{id}"),
            _ => self.obs_id.clone(),
        }
    }
}

fn view_value<'a>(surface: &'a PreparedSurfaceRecord, view: &str) -> &'a str {
    surface
        .normalized_views
        .get(view)
        .map_or("", |view| view.value.as_str())
}

fn anchor_text(surface: &PreparedSurfaceRecord, field: &str) -> String {
    let mut values = surface
        .anchors
        .iter()
        .filter(|anchor| anchor.namespace == field || anchor.field == field)
        .map(|anchor| anchor.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.join(",")
}

#[allow(clippy::too_many_arguments)]
fn assertion_line(
    kind: DerivedKind,
    operator_id: &str,
    namespace: &str,
    profile_hash: &str,
    left: &SurfaceInfo<'_>,
    right: &SurfaceInfo<'_>,
    steps: &[String],
    explanation: &str,
) -> Value {
    let (left, right) = if left.target(kind) <= right.target(kind) {
        (left, right)
    } else {
        (right, left)
    };
    let suffix = operator_id.split_once(':').map(|(_, rest)| rest);
    let (left_value, right_value) = match (kind, suffix) {
        (DerivedKind::NameEquivalent, Some(view)) => (
            view_value(left.record, view).to_string(),
            view_value(right.record, view).to_string(),
        ),
        (DerivedKind::SameKey | DerivedKind::Distinct, Some(field)) => (
            attribute_value(left.record, field),
            attribute_value(right.record, field),
        ),
        _ => (
            left.record.primary_surface.clone(),
            right.record.primary_surface.clone(),
        ),
    };
    let mut basis = Map::new();
    basis.insert("rule".to_string(), json!(operator_id));
    basis.insert("left_value".to_string(), json!(left_value));
    basis.insert("right_value".to_string(), json!(right_value));
    basis.insert("steps".to_string(), json!(steps));
    basis.insert("applies_to".to_string(), json!("surface"));
    basis.insert("explanation".to_string(), json!(explanation));
    if let (DerivedKind::NameEquivalent, Some(view)) = (kind, suffix) {
        let left_tail = left
            .record
            .normalized_views
            .get(view)
            .and_then(|normalized| normalized.removed_tail.clone());
        let right_tail = right
            .record
            .normalized_views
            .get(view)
            .and_then(|normalized| normalized.removed_tail.clone());
        if left_tail.is_some() || right_tail.is_some() {
            basis.insert("left_removed_tail".to_string(), json!(left_tail));
            basis.insert("right_removed_tail".to_string(), json!(right_tail));
        }
    }
    match kind {
        DerivedKind::SameKey => {
            if let Some(field) = suffix {
                basis.insert("namespace".to_string(), json!(field));
            }
        }
        DerivedKind::Distinct => {
            basis.insert(
                "authority".to_string(),
                json!("authoritative_incompatibility"),
            );
        }
        _ => {}
    }
    json!({
        "type": kind.evidence_type(),
        "left": left.target(kind),
        "right": right.target(kind),
        "basis": Value::Object(basis),
        "source": {"channel": "canon", "id": format!("derive:{operator_id}")},
        "asserted_by": format!("canon:{namespace}:{operator_id}@{profile_hash}"),
    })
}

/// Operators the profile declares that derive would silently ignore are
/// refused by name. Relation hints are the exception: the profile validator
/// requires the lane to be declared, no run executes them per operator, and
/// derive reports them instead.
fn split_inert(unwired: Vec<String>) -> (Vec<String>, Vec<String>) {
    unwired
        .into_iter()
        .partition(|label| !label.starts_with("relation_hints:"))
}

/// The value a surface holds for a declared attribute field: its normalized
/// view of that name, else an anchor of that name.
fn attribute_value(surface: &PreparedSurfaceRecord, field: &str) -> String {
    let view = view_value(surface, field).trim();
    if !view.is_empty() {
        return view.to_string();
    }
    anchor_text(surface, field)
}

/// One `attribute_agreement` record for a pair, listing every attribute the
/// pair agrees on: field, both values, the operator and its tolerance. Three
/// agreeing attributes are one evidence type, never three records.
fn attribute_agreement_line(
    hits: &[&crate::entity::edge::EdgeEvidenceHit],
    support: &[crate::entity::profile::EntityOperatorSpec],
    profile_hash: &str,
    left: &SurfaceInfo<'_>,
    right: &SurfaceInfo<'_>,
) -> Value {
    let (left, right) = if left.obs_id <= right.obs_id {
        (left, right)
    } else {
        (right, left)
    };
    let mut attributes = hits
        .iter()
        .map(|hit| {
            let (operator, field) = hit
                .operator_id
                .split_once(':')
                .unwrap_or((hit.operator_id.as_str(), ""));
            let spec = support.iter().find(|spec| {
                spec.op == operator
                    && (spec.params.get("field").map(String::as_str) == Some(field)
                        || spec.view.as_deref() == Some(field))
            });
            let tolerance = match (operator, spec) {
                ("numeric_within_tolerance", Some(spec)) => json!({
                    "absolute": spec.params.get("absolute").cloned().unwrap_or_else(|| "0".to_string()),
                    "relative_bps": spec.params.get("relative_bps").cloned(),
                }),
                _ => Value::Null,
            };
            json!({
                "field": field,
                "operator": operator,
                "left_value": attribute_value(left.record, field),
                "right_value": attribute_value(right.record, field),
                "tolerance": tolerance,
            })
        })
        .collect::<Vec<_>>();
    attributes.sort_by_key(|attribute| attribute["field"].as_str().unwrap_or_default().to_string());
    attributes.dedup();
    let fields = attributes
        .iter()
        .filter_map(|attribute| attribute["field"].as_str())
        .collect::<Vec<_>>();
    let rule = format!("attribute_agreement:{}", fields.join("+"));
    let namespace = hits.first().map_or("", |hit| hit.namespace.as_str());
    json!({
        "type": "attribute_agreement",
        "left": left.obs_id,
        "right": right.obs_id,
        "basis": {
            "rule": rule,
            "fields": fields,
            "attributes": attributes,
            "applies_to": "surface",
            "explanation": hits
                .iter()
                .map(|hit| hit.explanation.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        },
        "source": {"channel": "canon", "id": format!("derive:{rule}")},
        "asserted_by": format!("canon:{namespace}:{rule}@{profile_hash}"),
    })
}

pub fn derive(request: SessionDeriveRequest) -> Result<SessionDeriveOutput, Refusal> {
    let session = if request.session.is_absolute() {
        request.session.clone()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&request.session))
            .map_err(|error| {
                explain_refusal(
                    format!("Unable to resolve the session path: {error}"),
                    json!({"writes_performed": false}),
                    "Pass an absolute session path",
                )
            })?
    };
    let manifest = read_manifest(&session)?;
    let registry = manifest.registries.first().ok_or_else(|| {
        explain_refusal(
            "The session manifest has no registry binding",
            json!({"writes_performed": false}),
            "Initialize the session with a registry, then rerun derive",
        )
    })?;

    let loaded = load_prepare_profile_with_hash(&request.profile)?;
    let (mut inert, relation_hints_not_executed) =
        split_inert(unwired_run_operators(&loaded.document));
    // derive runs same_slot itself; `entity run` does not
    inert.retain(|label| label != &format!("support:{SAME_SLOT_OPERATOR}"));
    let slot_rules = slot_rules(&loaded.document)?;
    if !inert.is_empty() {
        return Err(Refusal {
            code: RefusalCode::EParse,
            message: format!(
                "The profile declares operators session derive does not execute: {}",
                inert.join(", ")
            ),
            detail: json!({"operators": inert, "writes_performed": false}),
            next_command: Some(
                "Remove the operators from the profile, or run them outside derive".to_string(),
            ),
        });
    }
    let profile_hash = loaded.content_hash.clone();
    if request.registry_candidates
        && !loaded.document.prepare.as_ref().is_some_and(|mapping| {
            mapping
                .provenance_fields
                .iter()
                .any(|field| field == "origin")
        })
    {
        return Err(Refusal {
            code: RefusalCode::EParse,
            message: "The profile must list origin in prepare.provenance_fields so block can tell registry rows from session rows".to_string(),
            detail: json!({"field": "prepare.provenance_fields", "writes_performed": false}),
            next_command: Some(
                "Add origin to the profile's prepare.provenance_fields, or pass --no-registry-candidates"
                    .to_string(),
            ),
        });
    }

    let snapshot = open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
        .map_err(|error| {
            explain_refusal(
                format!("No committed session observations are available: {error}"),
                json!({"writes_performed": false}),
                "Run canon entity session ingest, then rerun derive",
            )
        })?;
    let mut observations = shard_lines(&snapshot, "observations/")
        .into_iter()
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .collect::<Vec<_>>();
    observations.sort_by(|left, right| {
        left.get("obs_id")
            .and_then(Value::as_str)
            .cmp(&right.get("obs_id").and_then(Value::as_str))
    });
    observations.dedup_by(|left, right| left.get("obs_id") == right.get("obs_id"));
    if observations.is_empty() {
        return Err(explain_refusal(
            "The session has no observations to derive evidence from",
            json!({"writes_performed": false}),
            "Run canon entity session ingest, then rerun derive",
        ));
    }
    let candidate_rows = if request.registry_candidates {
        registry_rows(&manifest.registries)?
    } else {
        Vec::new()
    };
    let mut entity_types = observations
        .iter()
        .filter_map(|observation| {
            Some((
                observation.get("obs_id")?.as_str()?.to_string(),
                observation.get("entity_type")?.as_str()?.to_string(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let canonical_ids = candidate_rows
        .iter()
        .map(|row| (row.row_id(), row.canonical_id.clone()))
        .collect::<BTreeMap<_, _>>();
    for row in &candidate_rows {
        entity_types.insert(row.row_id(), row.entity_type.clone());
    }

    // The existing stages run on a rows file the session owns.
    let work_dir = session.join(DERIVE_WORK_DIR);
    if work_dir.exists() {
        fs::remove_dir_all(&work_dir).map_err(|error| {
            explain_refusal(
                format!("Unable to reset {}: {error}", work_dir.display()),
                json!({"writes_performed": false}),
                "Remove the derive-work directory, then rerun",
            )
        })?;
    }
    fs::create_dir_all(&work_dir).map_err(|error| {
        explain_refusal(
            format!("Unable to create {}: {error}", work_dir.display()),
            json!({"writes_performed": false}),
            "Check the session directory is writable",
        )
    })?;
    let rows_path = work_dir.join("rows.csv");
    let row_order = write_rows(&rows_path, &observations, &candidate_rows)?;
    let stage_dir = work_dir.join("stages");
    let result = run_entity_workbench(EntityRunRequest {
        rows: &rows_path,
        profile: &request.profile,
        strategy: &request.strategy,
        registry: Path::new(&registry.path),
        work_dir: &stage_dir,
    })?;
    let layout = &result.artifact.work_dir;

    let surfaces: Vec<PreparedSurfaceRecord> =
        read_jsonl(&stage_dir.join(&layout.surfaces_path), "prepared surfaces")?;
    let edge_records: Vec<EdgeEvidenceRecord> = read_jsonl(
        &stage_dir.join(&layout.edge_records_path),
        "evidence records",
    )?;
    let buckets: Vec<ExactBucketAssertion> = read_jsonl(
        &stage_dir.join(&layout.exact_bucket_assertions_path),
        "exact bucket assertions",
    )
    .unwrap_or_default();
    let prepare_value: Value = fs::read(stage_dir.join(&layout.prepare_artifact_path))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);

    let mut by_surface = BTreeMap::<String, SurfaceInfo<'_>>::new();
    for surface in &surfaces {
        let obs_id = surface
            .provenance_samples
            .iter()
            .filter_map(|sample| sample.source_row_id.clone())
            .min()
            .ok_or_else(|| {
                explain_refusal(
                    format!(
                        "Surface {} carries no source_row_id; the profile must list source_row_id in prepare.provenance_fields",
                        surface.surface_id
                    ),
                    json!({"surface_id": surface.surface_id, "writes_performed": false}),
                    "Add source_row_id to the profile's prepare.provenance_fields",
                )
            })?;
        by_surface.insert(
            surface.surface_id.clone(),
            SurfaceInfo {
                entity_type: entity_types.get(&obs_id).cloned().unwrap_or_default(),
                canonical_id: canonical_ids.get(&obs_id).cloned(),
                obs_id,
                record: surface,
            },
        );
    }

    let document = &loaded.document;

    // Person and trust names: classify, reject fragments, set effective types.
    let name_views = person_views(document);
    let mut name_stats = None;
    let surface_text = observations
        .iter()
        .filter_map(|observation| {
            Some((
                observation.get("obs_id")?.as_str()?.to_string(),
                observation.get("surface")?.as_str()?.to_string(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    if !name_views.is_empty() {
        let mut stats = NameStageStats::default();
        let mut fragment_obs = BTreeSet::new();
        for (obs_id, surface) in &surface_text {
            let declared = entity_types.get(obs_id).map_or("", String::as_str);
            let (class, fragment) = classify_observation(declared, surface);
            *stats.classified.entry(class).or_default() += 1;
            if fragment {
                fragment_obs.insert(obs_id.clone());
                stats.fragments_rejected.push(FragmentRejection {
                    obs_id: obs_id.clone(),
                    surface: surface.clone(),
                });
            }
        }
        by_surface.retain(|_, info| !fragment_obs.contains(&info.obs_id));
        for info in by_surface.values_mut() {
            if let Some(text) = surface_text.get(&info.obs_id) {
                info.entity_type = effective_type(&info.entity_type, text);
            }
        }
        name_stats = Some(stats);
    }
    // Alternates and member lists: only the surfaces that parse into a list are
    // needed ahead of the pairwise loop, so a generic hit never merges a list.
    let structure_view_names = structure_views(document);
    let structure_nodes = if structure_view_names.is_empty() {
        Vec::new()
    } else {
        by_surface
            .values()
            .filter(|info| !info.is_registry())
            .map(|info| StructureNode {
                obs_id: info.obs_id.as_str(),
                surface: surface_text
                    .get(&info.obs_id)
                    .map_or(info.record.primary_surface.as_str(), String::as_str),
                entity_type: info.entity_type.as_str(),
            })
            .collect::<Vec<_>>()
    };
    let list_obs = multi_party_ids(&structure_nodes);
    let mut structure_deferred = 0_usize;
    let steps_for = |view: &str, left: &SurfaceInfo<'_>, right: &SurfaceInfo<'_>| {
        let mut steps = document
            .normalized_views
            .get(view)
            .map(|view| view.operators.clone())
            .unwrap_or_default();
        let mut normalization_steps = BTreeSet::new();
        for surface in [left, right] {
            if let Some(normalized) = surface.record.normalized_views.get(view) {
                normalization_steps.extend(normalized.reason_codes.iter().cloned());
            }
        }
        for step in normalization_steps {
            if !steps.contains(&step) {
                steps.push(step);
            }
        }
        steps
    };

    let mut lines = BTreeMap::<String, Value>::new();
    let mut cross_type_skipped = 0;
    fn push(line: Value, lines: &mut BTreeMap<String, Value>) {
        lines.insert(line.to_string(), line);
    }
    for record in &edge_records {
        let (Some(left), Some(right)) = (
            by_surface.get(&record.left_surface_id),
            by_surface.get(&record.right_surface_id),
        ) else {
            continue;
        };
        if left.obs_id == right.obs_id || (left.is_registry() && right.is_registry()) {
            continue;
        }
        let mut agreeing = Vec::<&crate::entity::edge::EdgeEvidenceHit>::new();
        for hit in &record.hits {
            let Some(kind) = kind_for_hit(hit.lane, &hit.operator_id) else {
                continue;
            };
            if left.entity_type != right.entity_type {
                cross_type_skipped += 1;
                continue;
            }
            // People are compared by their parsed components below, never by a
            // generic name hit (a person view, or similarity between people).
            if let Some(stats) = name_stats.as_mut()
                && (hit
                    .operator_id
                    .split_once(':')
                    .is_some_and(|(_, view)| name_views.contains(view))
                    || (left.entity_type == "person"
                        && matches!(kind, DerivedKind::NameEquivalent | DerivedKind::NameSimilar)))
            {
                stats.generic_hits_deferred += 1;
                continue;
            }
            if kind == DerivedKind::NameEquivalent
                && (list_obs.contains(&left.obs_id) || list_obs.contains(&right.obs_id))
            {
                structure_deferred += 1;
                continue;
            }
            if kind == DerivedKind::AttributeAgreement {
                agreeing.push(hit);
                continue;
            }
            let steps = match (kind, hit.operator_id.split_once(':')) {
                (DerivedKind::NameEquivalent, Some((_, view))) => steps_for(view, left, right),
                _ => Vec::new(),
            };
            push(
                assertion_line(
                    kind,
                    &hit.operator_id,
                    &hit.namespace,
                    &profile_hash,
                    left,
                    right,
                    &steps,
                    &hit.explanation,
                ),
                &mut lines,
            );
        }
        if !agreeing.is_empty() {
            push(
                attribute_agreement_line(
                    &agreeing,
                    &document.evidence.support,
                    &profile_hash,
                    left,
                    right,
                ),
                &mut lines,
            );
        }
    }

    if let (Some(stats), Some(view)) = (name_stats.as_mut(), name_views.iter().next()) {
        // Committed aliases are here too (when registry candidates are on), so
        // a new spelling of a committed person is compared with them.
        let nodes = by_surface
            .values()
            .map(|info| NameNode {
                obs_id: info.obs_id.as_str(),
                surface: surface_text
                    .get(&info.obs_id)
                    .map_or(info.record.primary_surface.as_str(), String::as_str),
                entity_type: info.entity_type.as_str(),
                registry: info.is_registry(),
            })
            .collect::<Vec<_>>();
        let view_steps = document
            .normalized_views
            .get(view)
            .map(|view| view.operators.clone())
            .unwrap_or_default();
        let (name_lines, pass) = name_assertions(
            &nodes,
            view,
            &view_steps,
            document.profile.as_str(),
            &profile_hash,
        );
        stats.person_pairs_compared = pass.person_pairs_compared;
        stats.trust_relations = pass.trust_relations;
        for name_line in name_lines {
            push(name_line, &mut lines);
        }
    }

    let mut structure_stats = None;
    if let Some(view) = structure_view_names.iter().next() {
        let (structure_lines, mut stats) = structure_assertions(
            &structure_nodes,
            view,
            document.profile.as_str(),
            &profile_hash,
        );
        stats.generic_hits_deferred = structure_deferred;
        for structure_line in structure_lines {
            push(structure_line, &mut lines);
        }
        structure_stats = Some(stats);
    }

    // Exact buckets are not expanded to pairs by block; a bucket whose view the
    // profile declares an exact_view support operator on becomes a star of
    // name_equivalent records around its lowest observation.
    let declared_views = document
        .evidence
        .support
        .iter()
        .filter(|spec| spec.op == "exact_view")
        .filter_map(|spec| spec.view.clone())
        .collect::<BTreeSet<_>>();
    let mut exact_bucket_ranges_skipped = 0;
    for bucket in &buckets {
        if !bucket.membership.surface_ranges.is_empty() {
            exact_bucket_ranges_skipped += 1;
        }
        let Some(view) = bucket.operator_id.split_once(':').map(|(_, view)| view) else {
            continue;
        };
        if !declared_views.contains(view) {
            continue;
        }
        let mut members = bucket
            .membership
            .surface_ids
            .iter()
            .filter_map(|id| by_surface.get(id))
            .collect::<Vec<_>>();
        members.sort_by(|left, right| left.obs_id.cmp(&right.obs_id));
        // A bare spelling may share this view with one form, but it must not
        // bridge two different forms.  Refuse the whole ambiguous bucket so
        // `Name`, `Name LLC`, and `Name Inc` cannot become one component by
        // way of the bare spelling.  The canonical-form view still emits
        // same-form variants independently.
        if view == "bare" {
            let distinct_legal_forms = members
                .iter()
                .filter_map(|member| member.record.normalized_views.get(view))
                .flat_map(|normalized| normalized.reason_codes.iter())
                .filter_map(|reason| reason.strip_prefix("legal_form:"))
                .filter(|form| *form != "bare_equivalence")
                .collect::<BTreeSet<_>>();
            if distinct_legal_forms.len() > 1 {
                continue;
            }
        }
        // The star's hub is the lowest session surface: a bucket is lifted to
        // session-to-member edges only, never registry-to-registry (two ids
        // sharing a normalised name would otherwise read as incumbent_conflict).
        let Some(hub_index) = members.iter().position(|member| !member.is_registry()) else {
            continue;
        };
        let hub = members.remove(hub_index);
        let rest = members;
        let operator_id = format!("exact_view:{view}");
        let namespace = document.profile.as_str();
        for member in rest {
            if hub.entity_type != member.entity_type {
                cross_type_skipped += 1;
                continue;
            }
            if list_obs.contains(&hub.obs_id) || list_obs.contains(&member.obs_id) {
                if let Some(stats) = structure_stats.as_mut() {
                    stats.generic_hits_deferred += 1;
                }
                continue;
            }
            if let Some(stats) = name_stats.as_mut()
                && hub.entity_type == "person"
            {
                stats.generic_hits_deferred += 1;
                continue;
            }
            push(
                assertion_line(
                    DerivedKind::NameEquivalent,
                    &operator_id,
                    namespace,
                    &profile_hash,
                    hub,
                    member,
                    &steps_for(view, hub, member),
                    "exact bucket",
                ),
                &mut lines,
            );
        }
    }

    // Slots are read from the observations, not from block's candidate pairs.
    let mut slot_stats = None;
    for fields in &slot_rules {
        let stats = slot_stats.get_or_insert_with(SlotStats::default);
        let nameable = observations.iter().filter(|observation| {
            name_views.is_empty()
                || observation
                    .get("surface")
                    .and_then(Value::as_str)
                    .is_none_or(|surface| !classify_observation("", surface).1)
        });
        for slot_line in slot_assertions(
            nameable,
            fields,
            document.profile.as_str(),
            &profile_hash,
            stats,
        ) {
            push(slot_line, &mut lines);
        }
    }

    // What the registry contributed, measured from the candidates block wrote.
    let registry_surfaces = by_surface
        .iter()
        .filter(|(_, info)| info.is_registry())
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    let candidate_records: Vec<crate::entity::block::BlockCandidateRecord> = read_jsonl(
        &stage_dir.join(&layout.candidate_records_path),
        "candidate records",
    )
    .unwrap_or_default();
    let registry_stats = RegistryCandidateStats {
        enabled: request.registry_candidates,
        rows: candidate_rows.len(),
        session_registry_pairs: candidate_records
            .iter()
            .filter(|pair| {
                registry_surfaces.contains(&pair.left_surface_id)
                    != registry_surfaces.contains(&pair.right_surface_id)
            })
            .count(),
        registry_registry_pairs_scored: candidate_records
            .iter()
            .filter(|pair| {
                registry_surfaces.contains(&pair.left_surface_id)
                    && registry_surfaces.contains(&pair.right_surface_id)
            })
            .count(),
    };
    let records_digest = crate::witness::hash_bytes(
        lines
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
            .as_bytes(),
    );

    let mut records_by_type = BTreeMap::<String, usize>::new();
    let mut records_by_operator = BTreeMap::<String, usize>::new();
    for line in lines.values() {
        *records_by_type
            .entry(line["type"].as_str().unwrap_or_default().to_string())
            .or_default() += 1;
        *records_by_operator
            .entry(
                line["basis"]["rule"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
            .or_default() += 1;
    }

    // Rows the prepare stage rejected, by obs_id.
    let rejected_observations = prepare_value
        .get("rejected_rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let obs_id = row
                .get("source_row_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    row.get("row_number")
                        .and_then(Value::as_u64)
                        .and_then(|number| number.checked_sub(1))
                        .and_then(|index| row_order.get(index as usize).cloned())
                })
                .unwrap_or_default();
            SessionDeriveRejection {
                obs_id,
                code: row
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                message: row
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }
        })
        .collect::<Vec<_>>();

    // Append through the assert path: the same canonical records an agent
    // would assert, idempotent by evidence id.
    let assertions_path = work_dir.join("assertions.jsonl");
    let text = lines
        .values()
        .map(|line| serde_json::to_string(line).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&assertions_path, format!("{text}\n")).map_err(|error| {
        explain_refusal(
            format!("Unable to write {}: {error}", assertions_path.display()),
            json!({"writes_performed": true}),
            "Rerun canon entity session derive",
        )
    })?;
    let asserted = assert_evidence(SessionAssertRequest {
        session: session.clone(),
        evidence: assertions_path,
    })?;

    Ok(SessionDeriveOutput {
        version: SESSION_DERIVE_VERSION.to_string(),
        profile: document.profile.clone(),
        profile_hash,
        observations: observations.len(),
        surfaces: surfaces.len(),
        records_by_type,
        records_by_operator,
        accepted: asserted.accepted,
        already_present: asserted.already_present,
        rejected_assertions: asserted.rejected.len(),
        rejected_observations,
        cross_type_skipped,
        names: name_stats,
        structure: structure_stats,
        slots: slot_stats,
        exact_bucket_ranges_skipped,
        relation_hints_not_executed,
        registry_candidates: registry_stats,
        records_digest,
        shard: asserted.shard,
        generation_id: asserted.generation_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_operator_maps_to_its_evidence_kind() {
        let cases = [
            (
                ScoreLane::Support,
                "exact_view:core",
                DerivedKind::NameEquivalent,
            ),
            (ScoreLane::Support, "anchor_match:cik", DerivedKind::SameKey),
            (
                ScoreLane::Support,
                "isin_cusip_arithmetic:isin",
                DerivedKind::SameKey,
            ),
            (
                ScoreLane::Support,
                "string_similarity:core",
                DerivedKind::NameSimilar,
            ),
            (ScoreLane::Support, "tfidf_cosine", DerivedKind::NameSimilar),
            (
                ScoreLane::Support,
                "two_token_reversal:core",
                DerivedKind::NameSimilar,
            ),
            (
                ScoreLane::Support,
                "token_overlap:core",
                DerivedKind::NameSimilar,
            ),
            (
                ScoreLane::Support,
                "date_transposed_digits:maturity",
                DerivedKind::ValueSimilar,
            ),
            (
                ScoreLane::AntiMerge,
                "anchor_conflict:cik",
                DerivedKind::Distinct,
            ),
            (
                ScoreLane::AntiMerge,
                "attribute_conflict:spread",
                DerivedKind::Distinct,
            ),
            (
                ScoreLane::AntiMerge,
                "protected_token_conflict:core",
                DerivedKind::Distinct,
            ),
        ];
        for (lane, operator, expected) in cases {
            assert_eq!(kind_for_hit(lane, operator), Some(expected), "{operator}");
        }
        assert_eq!(
            kind_for_hit(ScoreLane::RelationHint, "cross_profile_alignment"),
            None
        );
        assert_eq!(kind_for_hit(ScoreLane::Support, "unknown_op"), None);
    }

    #[test]
    fn an_inert_support_or_cannot_link_operator_is_refused_but_relation_hints_are_reported() {
        let (refused, reported) = split_inert(vec![
            "support:frobnicate".to_string(),
            "cannot_link:mystery".to_string(),
            "relation_hints:division_of".to_string(),
        ]);
        assert_eq!(refused, ["support:frobnicate", "cannot_link:mystery"]);
        assert_eq!(reported, ["relation_hints:division_of"]);
    }

    #[test]
    fn similarity_is_never_filed_under_equivalence() {
        for operator in [
            "string_similarity:core",
            "tfidf_cosine",
            "two_token_reversal:core",
            "date_transposed_digits:d",
        ] {
            let kind = kind_for_hit(ScoreLane::Support, operator).unwrap();
            assert!(
                matches!(kind.evidence_type(), "name_similar" | "value_similar"),
                "{operator} -> {}",
                kind.evidence_type()
            );
        }
        assert_eq!(
            DerivedKind::NameEquivalent.evidence_type(),
            "name_equivalent"
        );
    }
}
