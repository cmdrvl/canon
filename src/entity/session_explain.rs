#![forbid(unsafe_code)]

//! Agent-facing explanation for an evidence session.
//!
//! This module is deliberately a projection over the committed session
//! streams.  It does not re-run tier arithmetic: edge tiers and missing
//! conditions come from the solve artifact that the session already records.

use crate::{
    Refusal, RefusalCode,
    entity::{
        evidence_ir::{EvidenceRecord, EvidenceTarget},
        publication::{EntityPublicationSnapshot, open_current_stream_generation},
        session::{
            SESSION_SOLVE_STREAM_ID, SessionSolveArtifact, SessionSolveCluster,
            SessionSolveContradiction, SessionSolveRelation,
        },
    },
    session::{
        SESSION_COMMIT_STREAM_ID, SESSION_EVIDENCE_STREAM_ID, SESSION_MANIFEST_STREAM_ID,
        SESSION_OBSERVATIONS_STREAM_ID, SessionManifest,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const SESSION_EXPLAIN_VERSION: &str = "canon.entity.session.explain.v1";

pub(crate) const TIER_B_TYPES: &[&str] = &["same_slot", "attribute_agreement", "name_equivalent"];

#[derive(Debug, Clone)]
pub struct SessionExplainRequest {
    pub session: PathBuf,
    pub query: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainOutput {
    pub version: String,
    pub target: SessionExplainTarget,
    pub stale: bool,
    pub stale_reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<SessionExplainNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<SessionExplainCluster>,
    pub edges: Vec<SessionExplainEdge>,
    pub contradictions: Vec<SessionExplainContradiction>,
    pub relations: Vec<SessionExplainRelation>,
    pub to_move_up: Vec<SessionExplainNext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committed: Option<SessionExplainCommitted>,
    pub next_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainTarget {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainNode {
    pub surface: String,
    pub observation_ids: Vec<String>,
    pub lineages: Vec<String>,
    pub keys_seen: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainCluster {
    pub id: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incumbent_id: Option<String>,
    pub member_surfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainEdge {
    pub other_surface: String,
    pub evidence: Vec<SessionExplainEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainEvidence {
    pub evidence_id: String,
    #[serde(rename = "type")]
    pub evidence_type: String,
    pub asserted_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis: Option<Value>,
    pub lineages: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainContradiction {
    pub reason: String,
    pub surfaces: Vec<String>,
    pub obs_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainRelation {
    pub left_surface: String,
    pub right_surface: String,
    pub relation: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainNext {
    pub edge: String,
    pub needs: String,
    pub accepted_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExplainCommitted {
    pub registry_id: String,
    pub canonical_id: Option<String>,
    pub session_authority: Option<Value>,
    pub outcome_ref: Option<String>,
    pub closure_hash: Option<String>,
    pub distinct_counterparts: Vec<String>,
    pub units: Vec<String>,
    pub part_of_parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionWhatIfOutput {
    pub version: String,
    pub changes: Vec<SessionWhatIfChange>,
    pub rejected: Vec<SessionWhatIfRejected>,
    pub next_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionWhatIfChange {
    pub surface: String,
    pub before: SessionWhatIfState,
    pub after: SessionWhatIfState,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionWhatIfState {
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionWhatIfRejected {
    pub line: usize,
    pub kind: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ObservationView {
    pub(crate) obs_id: String,
    pub(crate) surface: String,
    pub(crate) lineages: BTreeSet<String>,
    pub(crate) keys: BTreeMap<String, BTreeSet<String>>,
    /// Scalar `context` values, trimmed.
    pub(crate) context: BTreeMap<String, String>,
    /// `as_of_date` / `as_of` from context or attrs, as written.
    pub(crate) as_of: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawObservation {
    #[serde(default)]
    obs_id: String,
    #[serde(default)]
    surface: String,
    #[serde(default)]
    keys: BTreeMap<String, Value>,
    #[serde(default)]
    attrs: BTreeMap<String, Value>,
    #[serde(default)]
    context: BTreeMap<String, Value>,
    #[serde(default)]
    source: RawSource,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawSource {
    #[serde(default)]
    lineage: Option<String>,
    #[serde(default)]
    ancestry: Option<RawAncestry>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawAncestry {
    #[serde(default)]
    status: String,
    #[serde(default)]
    origin_source_ids: Vec<String>,
}

pub fn explain(request: SessionExplainRequest) -> Result<SessionExplainOutput, Refusal> {
    let session = absolute_path(&request.session)?;
    let solve = read_solve(&session)?;
    let observations_snapshot = open_stream(&session, SESSION_OBSERVATIONS_STREAM_ID)?;
    let observations = read_observations(&observations_snapshot)?;
    let evidence = read_evidence(&session)?;
    let manifest = read_manifest(&session)?;
    let target = resolve_target(&request.query, &observations, &solve)?;
    let node = observations
        .iter()
        .filter(|observation| observation.surface == target.surface)
        .cloned()
        .collect::<Vec<_>>();
    let node = (!node.is_empty()).then(|| node_view(&target.surface, &node));
    let stale_reasons = stale_reasons(&session, &solve)?;
    if !stale_reasons.is_empty() {
        return Ok(SessionExplainOutput {
            version: SESSION_EXPLAIN_VERSION.to_string(),
            target: target.target,
            stale: true,
            stale_reasons,
            node,
            cluster: None,
            edges: Vec::new(),
            contradictions: Vec::new(),
            relations: Vec::new(),
            to_move_up: Vec::new(),
            committed: None,
            next_command: Some(format!(
                "canon entity session status {} --emit json",
                session.display()
            )),
        });
    }

    let cluster = matching_cluster(&solve, &target.surface);
    let records = merge_evidence(&solve, evidence);
    let edges = build_edges(&target.surface, &records, &observations, &solve);
    let to_move_up = edges
        .iter()
        .filter(|edge| edge.tier.as_deref() == Some("C"))
        .filter_map(|edge| next_for_edge(&target.surface, edge, &solve))
        .collect::<Vec<_>>();
    let contradictions = solve
        .contradictions
        .iter()
        .filter(|contradiction| contradiction.surfaces.contains(&target.surface))
        .map(SessionExplainContradiction::from)
        .collect::<Vec<_>>();
    let relations = solve
        .relations
        .iter()
        .filter(|relation| {
            relation.left_surface == target.surface
                || relation.right_surface == target.surface
                || relation.subject_surface == target.surface
                || relation.object_surface == target.surface
        })
        .map(SessionExplainRelation::from)
        .collect::<Vec<_>>();
    let committed = read_committed(&session, &manifest, cluster.as_ref())?;

    Ok(SessionExplainOutput {
        version: SESSION_EXPLAIN_VERSION.to_string(),
        target: target.target,
        stale: false,
        stale_reasons,
        node,
        cluster,
        edges,
        contradictions,
        relations,
        to_move_up,
        committed,
        next_command: Some(format!(
            "canon entity session status {} --emit json",
            session.display()
        )),
    })
}

pub fn what_if(session: &Path, rules: &Path, extra: &Path) -> Result<SessionWhatIfOutput, Refusal> {
    let source = absolute_path(session)?;
    let before = read_solve(&source)?;
    let extra_lines = fs::read_to_string(extra).map_err(|error| {
        explain_refusal(
            format!("Unable to read what-if input {}: {error}", extra.display()),
            json!({"path": extra.display().to_string(), "writes_performed": false}),
            "Provide a readable JSONL file, then rerun canon entity session solve --what-if",
        )
    })?;
    let scratch = scratch_copy(&source, &extra_lines)?;
    let mut observations = Vec::new();
    let mut evidence = Vec::new();
    let mut rejected = Vec::new();
    for (line_index, line) in extra_lines.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(line).map_err(|error| {
            explain_refusal(
                format!("what-if line {} is invalid JSON: {error}", line_index + 1),
                json!({"line": line_index + 1, "writes_performed": false}),
                "Fix the what-if JSONL, then rerun canon entity session solve --what-if",
            )
        })?;
        if value.get("obs_id").is_some() {
            observations.push(line.to_string());
        } else if value.get("type").is_some() {
            evidence.push(line.to_string());
        } else {
            rejected.push(SessionWhatIfRejected {
                line: line_index + 1,
                kind: "input".to_string(),
                code: "missing_discriminator".to_string(),
                message: "what-if rows need obs_id for ingest or type for assert".to_string(),
            });
        }
    }
    if !observations.is_empty() {
        let path = scratch.join("what-if-observations.jsonl");
        fs::write(&path, format!("{}\n", observations.join("\n"))).map_err(|error| {
            explain_refusal(
                format!("Unable to write what-if observations: {error}"),
                json!({"writes_performed": false}),
                "Use an accessible temporary directory, then rerun what-if",
            )
        })?;
        let output = crate::session::ingest(crate::session::SessionIngestRequest {
            session: scratch.clone(),
            observations: path,
        })?;
        rejected.extend(
            output
                .rejected
                .into_iter()
                .map(|row| SessionWhatIfRejected {
                    line: row.line,
                    kind: "observation".to_string(),
                    code: row.code,
                    message: row.message,
                }),
        );
    }
    if !evidence.is_empty() {
        let path = scratch.join("what-if-evidence.jsonl");
        fs::write(&path, format!("{}\n", evidence.join("\n"))).map_err(|error| {
            explain_refusal(
                format!("Unable to write what-if evidence: {error}"),
                json!({"writes_performed": false}),
                "Use an accessible temporary directory, then rerun what-if",
            )
        })?;
        let output = crate::session::assert_evidence(crate::session::SessionAssertRequest {
            session: scratch.clone(),
            evidence: path,
        })?;
        rejected.extend(
            output
                .rejected
                .into_iter()
                .map(|row| SessionWhatIfRejected {
                    line: row.line,
                    kind: "evidence".to_string(),
                    code: row.code,
                    message: row.message,
                }),
        );
    }
    let after = crate::entity::session::solve(crate::entity::session::SessionSolveRequest {
        session: scratch,
        rules: rules.to_path_buf(),
    })?
    .artifact;
    let changes = edge_changes(&before, &after);
    Ok(SessionWhatIfOutput {
        version: SESSION_EXPLAIN_VERSION.to_string(),
        changes,
        rejected,
        next_command: Some(format!(
            "canon entity session solve {} --rules {} --emit json",
            session.display(),
            rules.display()
        )),
    })
}

#[derive(Debug, Clone)]
struct ResolvedTarget {
    target: SessionExplainTarget,
    surface: String,
}

fn resolve_target(
    query: &str,
    observations: &[ObservationView],
    solve: &SessionSolveArtifact,
) -> Result<ResolvedTarget, Refusal> {
    let (kind, value) = if let Some(value) = query.strip_prefix("surface:") {
        ("surface", value.to_string())
    } else if let Some(value) = query.strip_prefix("cluster:") {
        ("cluster", value.to_string())
    } else if observations
        .iter()
        .any(|observation| observation.obs_id == query)
    {
        ("obs_id", query.to_string())
    } else {
        ("surface", query.to_string())
    };
    let surface = match kind {
        "obs_id" => observations
            .iter()
            .find(|observation| observation.obs_id == value)
            .map(|observation| observation.surface.clone()),
        "surface" => observations
            .iter()
            .find(|observation| observation.surface == value)
            .map(|observation| observation.surface.clone())
            .or_else(|| {
                solve
                    .clusters
                    .iter()
                    .find(|cluster| cluster.surfaces.iter().any(|surface| surface == &value))
                    .and_then(|cluster| {
                        cluster
                            .surfaces
                            .iter()
                            .find(|surface| *surface == &value)
                            .cloned()
                    })
            }),
        "cluster" => solve
            .clusters
            .iter()
            .find(|cluster| cluster.cluster_id == value)
            .and_then(|cluster| cluster.surfaces.first().cloned()),
        _ => None,
    };
    let Some(surface) = surface else {
        return Err(explain_refusal(
            format!("Session explain target '{query}' was not found"),
            json!({"query": query, "writes_performed": false}),
            "Run canon entity session status for available session state, then rerun explain",
        ));
    };
    Ok(ResolvedTarget {
        target: SessionExplainTarget {
            kind: kind.to_string(),
            value,
        },
        surface,
    })
}

fn node_view(surface: &str, observations: &[ObservationView]) -> SessionExplainNode {
    let mut observation_ids = BTreeSet::new();
    let mut lineages = BTreeSet::new();
    let mut keys = BTreeMap::<String, BTreeSet<String>>::new();
    for observation in observations {
        observation_ids.insert(observation.obs_id.clone());
        lineages.extend(observation.lineages.iter().cloned());
        for (namespace, values) in &observation.keys {
            keys.entry(namespace.clone())
                .or_default()
                .extend(values.iter().cloned());
        }
    }
    SessionExplainNode {
        surface: surface.to_string(),
        observation_ids: observation_ids.into_iter().collect(),
        lineages: lineages.into_iter().collect(),
        keys_seen: keys
            .into_iter()
            .map(|(namespace, values)| (namespace, values.into_iter().collect()))
            .collect(),
    }
}

fn matching_cluster(solve: &SessionSolveArtifact, surface: &str) -> Option<SessionExplainCluster> {
    solve
        .clusters
        .iter()
        .find(|cluster| cluster.surfaces.iter().any(|member| member == surface))
        .map(|cluster| SessionExplainCluster {
            id: cluster.cluster_id.clone(),
            outcome: cluster.outcome.clone(),
            tier: cluster.tier.clone(),
            incumbent_id: cluster.incumbent_canonical_id.clone(),
            member_surfaces: cluster.surfaces.clone(),
        })
}

fn merge_evidence(
    solve: &SessionSolveArtifact,
    evidence: Vec<EvidenceRecord>,
) -> Vec<EvidenceRecord> {
    let mut records = BTreeMap::new();
    for record in solve.derived_evidence.iter().cloned().chain(evidence) {
        records.entry(record.evidence_id.clone()).or_insert(record);
    }
    records.into_values().collect()
}

fn build_edges(
    target_surface: &str,
    records: &[EvidenceRecord],
    observations: &[ObservationView],
    solve: &SessionSolveArtifact,
) -> Vec<SessionExplainEdge> {
    let by_id = observations
        .iter()
        .map(|observation| (observation.obs_id.as_str(), observation.surface.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut grouped = BTreeMap::<String, Vec<SessionExplainEvidence>>::new();
    for record in records {
        let EvidenceTarget::Pair { left_id, right_id } = &record.target else {
            continue;
        };
        let left_surface = by_id.get(left_id.as_str()).copied().unwrap_or(left_id);
        let right_surface = by_id.get(right_id.as_str()).copied().unwrap_or(right_id);
        let other = if left_surface == target_surface {
            Some(right_surface)
        } else if right_surface == target_surface {
            Some(left_surface)
        } else {
            None
        };
        let Some(other) = other else {
            continue;
        };
        let entry = SessionExplainEvidence {
            evidence_id: record.evidence_id.clone(),
            evidence_type: record.operator.operator_id.clone(),
            asserted_by: extension_string(record, "asserted_by"),
            basis: extension_value(record, "basis"),
            lineages: record_lineages(record),
        };
        grouped.entry(other.to_string()).or_default().push(entry);
    }
    grouped
        .into_iter()
        .map(|(other_surface, mut evidence)| {
            evidence.sort_by(|left, right| left.evidence_id.cmp(&right.evidence_id));
            let (tier, rule) = recorded_edge_state(target_surface, &other_surface, solve);
            SessionExplainEdge {
                other_surface,
                evidence,
                tier,
                rule,
            }
        })
        .collect()
}

fn recorded_edge_state(
    left: &str,
    right: &str,
    solve: &SessionSolveArtifact,
) -> (Option<String>, Option<String>) {
    if let Some(review) = solve.review.iter().find(|review| {
        (review.left_surface == left && review.right_surface == right)
            || (review.left_surface == right && review.right_surface == left)
    }) {
        return (
            Some("C".to_string()),
            Some(format!("tier_c: missing {}", review.missing)),
        );
    }
    if let Some(contradiction) = solve
        .contradictions
        .iter()
        .find(|contradiction| surfaces_match_pair(&contradiction.surfaces, left, right))
    {
        return (
            None,
            Some(format!("contradiction: {}", contradiction.reason)),
        );
    }
    if let Some(cluster) = solve
        .clusters
        .iter()
        .find(|cluster| surfaces_match_pair(&cluster.surfaces, left, right))
    {
        let rule = cluster.tier.as_deref().map(|tier| match tier {
            "A" => "tier_a.evidence_types".to_string(),
            "B" => "tier_b.evidence_types_and_lineages".to_string(),
            other => format!("tier_{other}"),
        });
        return (cluster.tier.clone(), rule);
    }
    (None, None)
}

fn next_for_edge(
    target_surface: &str,
    edge: &SessionExplainEdge,
    solve: &SessionSolveArtifact,
) -> Option<SessionExplainNext> {
    let review = solve.review.iter().find(|review| {
        (review.left_surface == target_surface && review.right_surface == edge.other_surface)
            || (review.left_surface == edge.other_surface && review.right_surface == target_surface)
    })?;
    let needs = review.missing.clone();
    Some(SessionExplainNext {
        edge: format!("{target_surface} <> {}", edge.other_surface),
        needs,
        accepted_types: TIER_B_TYPES
            .iter()
            .map(|kind| (*kind).to_string())
            .collect(),
    })
}

impl From<&SessionSolveContradiction> for SessionExplainContradiction {
    fn from(value: &SessionSolveContradiction) -> Self {
        Self {
            reason: value.reason.clone(),
            surfaces: value.surfaces.clone(),
            obs_ids: value.obs_ids.clone(),
            evidence_ids: value.evidence_ids.clone(),
        }
    }
}

impl From<&SessionSolveRelation> for SessionExplainRelation {
    fn from(value: &SessionSolveRelation) -> Self {
        Self {
            left_surface: value.left_surface.clone(),
            right_surface: value.right_surface.clone(),
            relation: value.relation.clone(),
            evidence_ids: value.evidence_ids.clone(),
        }
    }
}

fn read_solve(session: &Path) -> Result<SessionSolveArtifact, Refusal> {
    read_solve_hashed(session).map(|(solve, _)| solve)
}

/// The recorded solve and the hash of its `solve/solve.json` bytes.
pub(crate) fn read_solve_hashed(session: &Path) -> Result<(SessionSolveArtifact, String), Refusal> {
    let snapshot = open_stream(session, SESSION_SOLVE_STREAM_ID)?;
    let bytes = snapshot
        .read_logical_file("solve/solve.json")
        .ok_or_else(|| {
            explain_refusal(
                "Session solve generation is missing solve/solve.json",
                json!({"stream": SESSION_SOLVE_STREAM_ID, "writes_performed": false}),
                "Run canon entity session solve, then rerun explain",
            )
        })?;
    let solve_hash = crate::witness::hash_bytes(bytes);
    serde_json::from_slice(bytes)
        .map(|solve| (solve, solve_hash))
        .map_err(|error| {
            explain_refusal(
                format!("Session solve artifact is invalid: {error}"),
                json!({"stream": SESSION_SOLVE_STREAM_ID, "writes_performed": false}),
                "Run canon entity session solve, then rerun explain",
            )
        })
}

pub(crate) fn read_manifest(session: &Path) -> Result<SessionManifest, Refusal> {
    let snapshot = open_stream(session, SESSION_MANIFEST_STREAM_ID)?;
    let bytes = snapshot
        .read_logical_file("session/session.json")
        .ok_or_else(|| {
            explain_refusal(
                "Session manifest generation is missing session/session.json",
                json!({"stream": SESSION_MANIFEST_STREAM_ID, "writes_performed": false}),
                "Run canon entity session init, then rerun explain",
            )
        })?;
    serde_json::from_slice(bytes).map_err(|error| {
        explain_refusal(
            format!("Session manifest is invalid: {error}"),
            json!({"stream": SESSION_MANIFEST_STREAM_ID, "writes_performed": false}),
            "Run canon entity session init, then rerun explain",
        )
    })
}

pub(crate) fn read_observations(
    snapshot: &EntityPublicationSnapshot,
) -> Result<Vec<ObservationView>, Refusal> {
    let mut observations = BTreeMap::new();
    for path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot.read_logical_file(path).ok_or_else(|| {
            explain_refusal(
                format!("Observation shard {path} is missing"),
                json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                "Repair the observation stream, then rerun explain",
            )
        })?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            explain_refusal(
                format!("Observation shard {path} is not UTF-8: {error}"),
                json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                "Repair the observation shard, then rerun explain",
            )
        })?;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let raw = serde_json::from_str::<RawObservation>(line).map_err(|error| {
                explain_refusal(
                    format!("Observation shard {path} is invalid: {error}"),
                    json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                    "Repair the observation stream, then rerun explain",
                )
            })?;
            let lineages = raw_lineages(&raw.source);
            let context = raw
                .context
                .iter()
                .filter_map(|(name, value)| {
                    scalar_text(value)
                        .filter(|value| !value.is_empty())
                        .map(|value| (name.clone(), value))
                })
                .collect::<BTreeMap<_, _>>();
            let as_of = raw
                .context
                .get("as_of_date")
                .or_else(|| raw.context.get("as_of"))
                .or_else(|| raw.attrs.get("as_of_date"))
                .or_else(|| raw.attrs.get("as_of"))
                .and_then(scalar_text)
                .filter(|value| !value.is_empty());
            let keys = raw
                .keys
                .into_iter()
                .filter_map(|(namespace, value)| {
                    scalar_text(&value).map(|value| (namespace, value))
                })
                .map(|(namespace, value)| (namespace, BTreeSet::from([value])))
                .collect::<BTreeMap<_, _>>();
            observations.insert(
                raw.obs_id.clone(),
                ObservationView {
                    obs_id: raw.obs_id,
                    surface: raw.surface,
                    lineages,
                    keys,
                    context,
                    as_of,
                },
            );
        }
    }
    Ok(observations.into_values().collect())
}

fn read_evidence(session: &Path) -> Result<Vec<EvidenceRecord>, Refusal> {
    let Some(snapshot) = optional_stream(session, SESSION_EVIDENCE_STREAM_ID)? else {
        return Ok(Vec::new());
    };
    let mut records = BTreeMap::new();
    for path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot.read_logical_file(path).ok_or_else(|| {
            explain_refusal(
                format!("Evidence shard {path} is missing"),
                json!({"stream": SESSION_EVIDENCE_STREAM_ID, "writes_performed": false}),
                "Repair the evidence stream, then rerun explain",
            )
        })?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            explain_refusal(
                format!("Evidence shard {path} is not UTF-8: {error}"),
                json!({"stream": SESSION_EVIDENCE_STREAM_ID, "writes_performed": false}),
                "Repair the evidence shard, then rerun explain",
            )
        })?;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let record = serde_json::from_str::<EvidenceRecord>(line).map_err(|error| {
                explain_refusal(
                    format!("Evidence shard {path} is invalid: {error}"),
                    json!({"stream": SESSION_EVIDENCE_STREAM_ID, "writes_performed": false}),
                    "Repair the evidence stream, then rerun explain",
                )
            })?;
            records.entry(record.evidence_id.clone()).or_insert(record);
        }
    }
    Ok(records.into_values().collect())
}

pub(crate) fn stale_reasons(
    session: &Path,
    solve: &SessionSolveArtifact,
) -> Result<Vec<String>, Refusal> {
    let observations = open_stream(session, SESSION_OBSERVATIONS_STREAM_ID)?.generation_id;
    let evidence = optional_stream(session, SESSION_EVIDENCE_STREAM_ID)?.map(|s| s.generation_id);
    let mut reasons = Vec::new();
    if observations != solve.inputs.observations_generation {
        reasons.push("observations_generation".to_string());
    }
    if evidence != solve.inputs.evidence_generation {
        reasons.push("evidence_generation".to_string());
    }
    Ok(reasons)
}

fn read_committed(
    session: &Path,
    manifest: &SessionManifest,
    cluster: Option<&SessionExplainCluster>,
) -> Result<Option<SessionExplainCommitted>, Refusal> {
    let Some(cluster) = cluster else {
        return Ok(None);
    };
    let Some(snapshot) = optional_stream(session, SESSION_COMMIT_STREAM_ID)? else {
        return Ok(None);
    };
    let Some(bytes) = snapshot.read_logical_file("commit/commit.json") else {
        return Ok(None);
    };
    let receipt = serde_json::from_slice::<Value>(bytes).map_err(|error| {
        explain_refusal(
            format!("Session commit receipt is invalid: {error}"),
            json!({"stream": SESSION_COMMIT_STREAM_ID, "writes_performed": false}),
            "Repair the commit stream, then rerun explain",
        )
    })?;
    let item = receipt
        .get("minted")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            receipt
                .get("attached")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        )
        .find(|item| item.get("cluster_id").and_then(Value::as_str) == Some(cluster.id.as_str()));
    let Some(item) = item else {
        return Ok(None);
    };
    let canonical_id = item
        .get("canonical_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let session_authority = item
        .get("session_authority")
        .cloned()
        .or_else(|| registry_session_authority(manifest, canonical_id.as_deref()));
    let distinct_counterparts = manifest
        .registries
        .iter()
        .flat_map(|binding| read_memory_pairs(Path::new(&binding.path)))
        .filter_map(|(left, right)| {
            let incumbent = cluster.incumbent_id.as_deref()?;
            if left == incumbent {
                Some(right)
            } else if right == incumbent {
                Some(left)
            } else {
                None
            }
        })
        .collect();
    Ok(Some(SessionExplainCommitted {
        registry_id: manifest
            .registries
            .first()
            .map(|binding| binding.id.clone())
            .unwrap_or_default(),
        canonical_id,
        session_authority,
        outcome_ref: item
            .get("outcome_ref")
            .and_then(Value::as_str)
            .map(str::to_string),
        closure_hash: item
            .get("closure_hash")
            .and_then(Value::as_str)
            .map(str::to_string),
        distinct_counterparts,
        units: Vec::new(),
        part_of_parents: Vec::new(),
    }))
}

fn registry_session_authority(
    manifest: &SessionManifest,
    canonical_id: Option<&str>,
) -> Option<Value> {
    let canonical_id = canonical_id?;
    manifest.registries.iter().find_map(|binding| {
        let alias_file = binding.alias_file.as_deref().unwrap_or("aliases.json");
        let path = Path::new(&binding.path).join(alias_file);
        let values = serde_json::from_slice::<Vec<Value>>(&fs::read(path).ok()?).ok()?;
        values.into_iter().find_map(|entry| {
            if entry.get("canonical_id").and_then(Value::as_str) == Some(canonical_id) {
                entry.get("session_authority").cloned()
            } else {
                None
            }
        })
    })
}

fn read_memory_pairs(registry: &Path) -> Vec<(String, String)> {
    let path = registry.join("_escrow/cannot_link.jsonl");
    fs::read_to_string(path)
        .ok()
        .into_iter()
        .flat_map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .collect::<Vec<_>>()
        })
        .filter_map(|value| {
            Some((
                value.get("left_key")?.as_str()?.to_string(),
                value.get("right_key")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn edge_changes(
    before: &SessionSolveArtifact,
    after: &SessionSolveArtifact,
) -> Vec<SessionWhatIfChange> {
    let mut pairs = BTreeSet::new();
    for review in &before.review {
        pairs.insert(ordered_surface_pair(
            &review.left_surface,
            &review.right_surface,
        ));
    }
    for review in &after.review {
        pairs.insert(ordered_surface_pair(
            &review.left_surface,
            &review.right_surface,
        ));
    }
    for cluster in &before.clusters {
        add_cluster_pairs(cluster, &mut pairs);
    }
    for cluster in &after.clusters {
        add_cluster_pairs(cluster, &mut pairs);
    }
    pairs
        .into_iter()
        .filter_map(|(left, right)| {
            let before_state = edge_state(before, &left, &right)?;
            let after_state = edge_state(after, &left, &right)?;
            (before_state.outcome != after_state.outcome || before_state.tier != after_state.tier)
                .then_some(SessionWhatIfChange {
                    surface: format!("{left} <> {right}"),
                    before: before_state,
                    after: after_state,
                })
        })
        .collect()
}

fn add_cluster_pairs(cluster: &SessionSolveCluster, pairs: &mut BTreeSet<(String, String)>) {
    for (index, left) in cluster.surfaces.iter().enumerate() {
        for right in cluster.surfaces.iter().skip(index + 1) {
            pairs.insert(ordered_surface_pair(left, right));
        }
    }
}

fn edge_state(solve: &SessionSolveArtifact, left: &str, right: &str) -> Option<SessionWhatIfState> {
    if solve.review.iter().any(|review| {
        ordered_surface_pair(&review.left_surface, &review.right_surface)
            == (left.to_string(), right.to_string())
    }) {
        return Some(SessionWhatIfState {
            outcome: "review".to_string(),
            tier: Some("C".to_string()),
        });
    }
    if solve
        .contradictions
        .iter()
        .any(|contradiction| surfaces_match_pair(&contradiction.surfaces, left, right))
    {
        return Some(SessionWhatIfState {
            outcome: "contradiction".to_string(),
            tier: None,
        });
    }
    solve
        .clusters
        .iter()
        .find(|cluster| surfaces_match_pair(&cluster.surfaces, left, right))
        .map(|cluster| SessionWhatIfState {
            outcome: cluster.outcome.clone(),
            tier: cluster.tier.clone(),
        })
}

fn surfaces_match_pair(surfaces: &[String], left: &str, right: &str) -> bool {
    surfaces.iter().any(|surface| surface == left)
        && surfaces.iter().any(|surface| surface == right)
}

fn ordered_surface_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn raw_lineages(source: &RawSource) -> BTreeSet<String> {
    if let Some(ancestry) = &source.ancestry
        && ancestry.status == "known"
    {
        let values = ancestry
            .origin_source_ids
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>();
        if !values.is_empty() {
            return values;
        }
    }
    source
        .lineage
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| BTreeSet::from([value.to_string()]))
        .unwrap_or_default()
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.trim().to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn extension_value(record: &EvidenceRecord, key: &str) -> Option<Value> {
    record.extensions.iter().find_map(|extension| {
        extension
            .payload
            .get(key)
            .and_then(|value| serde_json::to_value(value).ok())
    })
}

fn extension_string(record: &EvidenceRecord, key: &str) -> Option<String> {
    extension_value(record, key).and_then(|value| value.as_str().map(str::to_string))
}

fn record_lineages(record: &EvidenceRecord) -> Vec<String> {
    let mut lineages = BTreeSet::new();
    if let Some(value) = extension_string(record, "lineage") {
        lineages.insert(value);
    }
    if let Some(Value::Object(ancestry)) = extension_value(record, "ancestry")
        && let Some(Value::Array(ids)) = ancestry.get("origin_source_ids")
    {
        lineages.extend(ids.iter().filter_map(Value::as_str).map(str::to_string));
    }
    lineages.into_iter().collect()
}

pub(crate) fn open_stream(
    session: &Path,
    stream: &str,
) -> Result<EntityPublicationSnapshot, Refusal> {
    open_current_stream_generation(session, stream).map_err(|error| {
        explain_refusal(
            format!("No committed {stream} generation is available: {error}"),
            json!({"stream": stream, "writes_performed": false}),
            format!("Run the session {stream} command, then rerun explain"),
        )
    })
}

fn optional_stream(
    session: &Path,
    stream: &str,
) -> Result<Option<EntityPublicationSnapshot>, Refusal> {
    match open_current_stream_generation(session, stream) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error)
            if error.kind
                == crate::entity::publication::EntityPublicationErrorKind::UncommittedGeneration =>
        {
            Ok(None)
        }
        Err(error) => Err(explain_refusal(
            format!("Unable to read session stream {stream}: {error}"),
            json!({"stream": stream, "writes_performed": false}),
            "Repair the session publication, then rerun explain",
        )),
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf, Refusal> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| {
                explain_refusal(
                    format!(
                        "Unable to resolve session path '{}': {error}",
                        path.display()
                    ),
                    json!({"path": path.display().to_string(), "writes_performed": false}),
                    "Use an accessible session work directory, then rerun explain",
                )
            })
    }
}

fn scratch_copy(session: &Path, input: &str) -> Result<PathBuf, Refusal> {
    let digest = blake3::hash(input.as_bytes()).to_hex().to_string();
    let base = std::env::temp_dir().join(format!(
        "canon-session-what-if-{}-{}",
        std::process::id(),
        &digest[..16]
    ));
    let mut scratch = base.clone();
    let mut created = false;
    for suffix in 0..1000u32 {
        if suffix > 0 {
            scratch = base.with_file_name(format!(
                "{}-{suffix}",
                base.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("session")
            ));
        }
        match fs::create_dir(&scratch) {
            Ok(()) => {
                created = true;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(explain_refusal(
                    format!("Unable to create what-if workspace: {error}"),
                    json!({"writes_performed": false}),
                    "Use an accessible temporary directory, then rerun what-if",
                ));
            }
        }
    }
    if !created {
        return Err(explain_refusal(
            "Unable to allocate a unique what-if workspace",
            json!({"writes_performed": false}),
            "Retry the what-if command",
        ));
    }
    copy_tree(session, &scratch)?;
    Ok(scratch)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), Refusal> {
    for entry in fs::read_dir(source).map_err(|error| {
        explain_refusal(
            format!("Unable to read session for what-if: {error}"),
            json!({"writes_performed": false}),
            "Use an accessible session work directory, then rerun what-if",
        )
    })? {
        let entry = entry.map_err(|error| {
            explain_refusal(
                format!("Unable to inspect session for what-if: {error}"),
                json!({"writes_performed": false}),
                "Use an accessible session work directory, then rerun what-if",
            )
        })?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            fs::create_dir_all(&destination_path).map_err(|error| {
                explain_refusal(
                    format!("Unable to copy what-if directory: {error}"),
                    json!({"writes_performed": false}),
                    "Use an accessible temporary directory, then rerun what-if",
                )
            })?;
            copy_tree(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                explain_refusal(
                    format!("Unable to copy what-if file: {error}"),
                    json!({"writes_performed": false}),
                    "Use an accessible temporary directory, then rerun what-if",
                )
            })?;
        }
    }
    Ok(())
}

pub(crate) fn explain_refusal(
    message: impl Into<String>,
    detail: Value,
    next_command: impl Into<String>,
) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.into(),
        detail,
        next_command: Some(next_command.into()),
    }
}
