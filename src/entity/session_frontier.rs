#![forbid(unsafe_code)]

//! The session frontier: what the recorded solve still leaves unresolved, as
//! data, and the evidence kinds that would settle each part of it.
//!
//! Like explain, this is a projection over the committed session streams. It
//! re-uses the solve's own `missing` values and explain's observation reader;
//! it does not re-run tier arithmetic and it names evidence types and scopes,
//! never commands, queries or a ranking. Optional lease claims are stored as
//! append-only publication records and do not alter the solve or its frontier.
//! `settles_with` is conditional on the evidence existing: canon does not know
//! that it does.

use crate::{
    Refusal,
    entity::{
        publication::{
            EntityPublicationErrorKind, EntityPublicationFileInput, EntityPublicationRequest,
            EntityPublicationUpstreamRef, open_current_stream_generation, publish_stream_patch,
        },
        session::{SESSION_SOLVE_STREAM_ID, SessionSolveArtifact, SessionSolveCluster},
        session_explain::{
            ObservationView, TIER_B_TYPES, explain_refusal, open_stream, read_observations,
            read_solve_hashed, stale_reasons,
        },
    },
    session::{SESSION_MANIFEST_STREAM_ID, SESSION_OBSERVATIONS_STREAM_ID},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const SESSION_FRONTIER_VERSION: &str = "canon.entity.session.frontier.v1";
pub const SESSION_FRONTIER_CLAIMS_VERSION: &str = "canon.entity.session.frontier.claims.v1";

pub const SESSION_FRONTIER_CLAIMS_STREAM_ID: &str = "entity-session-frontier-claims";
const SESSION_FRONTIER_CLAIMS_RECORDS_PATH: &str = "frontier/leases";
const SESSION_FRONTIER_CLAIMS_MAX_ATTEMPTS: usize = 32;

const NEEDS_AUTHORITATIVE_KEY: &str = "needs_authoritative_key";
const NEEDS_SECOND_EVIDENCE_TYPE: &str = "needs_second_evidence_type";
const NEEDS_INDEPENDENT_ORIGIN: &str = "needs_independent_origin";
const NEEDS_HUMAN: &str = "needs_human";
const UNKNOWN_SOURCE: &str = "<unknown>";

#[derive(Debug, Clone)]
pub struct SessionFrontierRequest {
    pub session: PathBuf,
    pub seed_entity: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontierShard {
    index: u64,
    total: u64,
}

impl FrontierShard {
    pub fn parse(value: &str) -> Result<Self, String> {
        let (index, total) = value
            .split_once('/')
            .ok_or_else(|| "shard must be written as INDEX/TOTAL".to_string())?;
        let index = index
            .parse::<u64>()
            .map_err(|_| "shard index must be a non-negative integer".to_string())?;
        let total = total
            .parse::<u64>()
            .map_err(|_| "shard total must be a positive integer".to_string())?;
        if total == 0 || index >= total {
            return Err("shard index must be smaller than its positive total".to_string());
        }
        Ok(Self { index, total })
    }
}

impl std::str::FromStr for FrontierShard {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionFrontierOutput {
    pub version: String,
    pub solve_hash: String,
    pub coverage: Vec<FrontierCoverage>,
    pub unresolved_keys: Vec<FrontierUnresolvedKey>,
    pub components: Vec<FrontierComponent>,
    /// Stable, bounded work payloads suitable for handing to an agent.
    pub work_items: Vec<FrontierWorkItem>,
    pub terminal: Vec<FrontierTerminal>,
    pub history_requests: Vec<FrontierHistoryRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<FrontierSeed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition: Option<FrontierPartition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierPartition {
    pub index: u64,
    pub total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierWorkItem {
    pub frontier_id: String,
    pub missing: String,
    pub surfaces: Vec<String>,
    pub observations: Vec<FrontierObservation>,
    pub observations_truncated: bool,
    pub incumbent_candidates: Vec<String>,
    pub existing_partial_evidence: Vec<String>,
    pub cannot_link_vetoes: Vec<FrontierCannotLinkVeto>,
    pub needed_evidence_kinds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FrontierCannotLinkVeto {
    pub reason: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierObservation {
    pub obs_id: String,
    pub surface: String,
    pub lineages: Vec<String>,
    pub keys: BTreeMap<String, Vec<String>>,
    pub details_truncated: bool,
}

#[derive(Debug, Clone)]
pub struct SessionFrontierClaimRequest {
    pub session: PathBuf,
    pub worker: String,
    pub limit: usize,
    pub ttl_secs: u64,
}

#[derive(Debug, Clone)]
pub struct SessionFrontierReleaseRequest {
    pub session: PathBuf,
    pub claim_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionFrontierClaimOutput {
    pub version: String,
    pub solve_hash: String,
    pub claim_id: Option<String>,
    pub worker: String,
    pub expires_at_unix_secs: Option<u64>,
    pub frontier_ids: Vec<String>,
    pub work_items: Vec<FrontierWorkItem>,
    pub unresolved_keys: Vec<FrontierUnresolvedKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionFrontierReleaseOutput {
    pub version: String,
    pub claim_id: String,
    pub released: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum FrontierClaimEvent {
    Claim {
        claim_id: String,
        worker: String,
        solve_hash: String,
        frontier_ids: Vec<String>,
        expires_at_unix_secs: u64,
    },
    Release {
        claim_id: String,
        released_at_unix_secs: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierCoverage {
    pub key_family: String,
    pub source: String,
    pub attempted: usize,
    pub supported: usize,
    pub conflicted: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierUnresolvedKey {
    pub frontier_id: String,
    pub namespace: String,
    pub value: String,
    pub surfaces: Vec<String>,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierComponent {
    pub group_id: String,
    pub missing: String,
    pub settles_with: Vec<String>,
    pub items: usize,
    pub surfaces: Vec<String>,
    pub observation_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierTerminal {
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierHistoryRequest {
    pub group_id: String,
    pub need: String,
    pub scope: BTreeMap<String, String>,
    pub before: Option<String>,
    pub settles_with: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierSeed {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    pub neighbourhood: Vec<String>,
    pub unresolved_keys: Vec<FrontierSeedKey>,
    pub unresolved_spellings: Vec<FrontierSeedSpelling>,
    pub unresolved_units: Vec<FrontierSeedUnit>,
    pub history_requests: Vec<FrontierHistoryRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierSeedKey {
    pub entity: String,
    pub namespace: String,
    pub settles_with: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierSeedSpelling {
    pub surface: String,
    pub group_id: String,
    pub settles_with: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontierSeedUnit {
    pub surface: String,
    pub settles_with: Vec<String>,
}

impl SessionFrontierOutput {
    /// `session next`: the same data, one plain line per fact. Every number and
    /// name printed here is a field of the JSON.
    pub fn render_summary(&self) -> String {
        let mut lines = vec![format!(
            "frontier solve_hash={} groups={}",
            self.solve_hash,
            self.components.len()
        )];
        for group in &self.components {
            lines.push(format!(
                "group id={} items={} surfaces={} observations={} settles_with={}",
                group.group_id,
                group.items,
                group.surfaces.len(),
                group.observation_count,
                group.settles_with.join(",")
            ));
        }
        for row in &self.coverage {
            lines.push(format!(
                "coverage key_family={} source={} attempted={} supported={} conflicted={} unresolved={}",
                row.key_family,
                quoted(&row.source),
                row.attempted,
                row.supported,
                row.conflicted,
                row.unresolved
            ));
        }
        for key in &self.unresolved_keys {
            lines.push(format!(
                "unresolved_key namespace={} value={} surfaces={}",
                key.namespace,
                quoted(&key.value),
                key.surfaces.len()
            ));
        }
        for row in &self.terminal {
            lines.push(format!(
                "terminal reason={} count={}",
                row.reason, row.count
            ));
        }
        for request in &self.history_requests {
            lines.push(history_line("history", request));
        }
        if let Some(seed) = &self.seed {
            lines.push(format!(
                "seed query={} neighbourhood={} unresolved_keys={} unresolved_spellings={} unresolved_units={} history_requests={}",
                quoted(&seed.query),
                seed.neighbourhood.len(),
                seed.unresolved_keys.len(),
                seed.unresolved_spellings.len(),
                seed.unresolved_units.len(),
                seed.history_requests.len()
            ));
            for key in &seed.unresolved_keys {
                lines.push(format!(
                    "seed_key entity={} namespace={}",
                    quoted(&key.entity),
                    key.namespace
                ));
            }
            for unit in &seed.unresolved_units {
                lines.push(format!("seed_unit surface={}", quoted(&unit.surface)));
            }
        }
        lines.join("\n")
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn history_line(label: &str, request: &FrontierHistoryRequest) -> String {
    let scope = request
        .scope
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "{label} group={} need={} before={} scope={} settles_with={}",
        request.group_id,
        request.need,
        request.before.as_deref().unwrap_or("none"),
        quoted(&scope),
        request.settles_with.join(",")
    )
}

pub fn frontier(request: SessionFrontierRequest) -> Result<SessionFrontierOutput, Refusal> {
    frontier_partitioned(request, None, None)
}

pub fn frontier_partitioned(
    request: SessionFrontierRequest,
    shard: Option<FrontierShard>,
    limit: Option<usize>,
) -> Result<SessionFrontierOutput, Refusal> {
    let session = absolute(&request.session)?;
    let (solve, solve_hash) = read_solve_hashed(&session)?;
    let stale = stale_reasons(&session, &solve)?;
    if !stale.is_empty() {
        return Err(explain_refusal(
            "Session solve is stale; the frontier is bound to a current solve",
            json!({
                "reason": "stale_solve",
                "stale_reasons": stale,
                "writes_performed": false,
            }),
            "Run canon entity session solve, then rerun frontier",
        ));
    }
    let snapshot = open_stream(&session, SESSION_OBSERVATIONS_STREAM_ID)?;
    let observations = read_observations(&snapshot)?;
    build_frontier(
        &solve,
        &solve_hash,
        &observations,
        request.seed_entity.as_deref(),
        shard,
        limit,
    )
}

pub fn claim_frontier(
    request: SessionFrontierClaimRequest,
) -> Result<SessionFrontierClaimOutput, Refusal> {
    if request.worker.trim().is_empty() || request.worker.chars().count() > 128 {
        return Err(explain_refusal(
            "Frontier worker id must contain 1 to 128 characters",
            json!({"writes_performed": false}),
            "Pass a non-empty --worker id of at most 128 characters",
        ));
    }
    if request.limit == 0 || request.ttl_secs == 0 {
        return Err(explain_refusal(
            "Frontier claim limit and TTL must be positive",
            json!({"limit": request.limit, "ttl_secs": request.ttl_secs, "writes_performed": false}),
            "Pass --limit and --ttl-secs values greater than zero",
        ));
    }
    let session = absolute(&request.session)?;
    let frontier = frontier_partitioned(
        SessionFrontierRequest {
            session: session.clone(),
            seed_entity: None,
        },
        None,
        None,
    )?;
    let solve_hash = frontier.solve_hash.clone();
    let mut frontier_ids = frontier
        .work_items
        .iter()
        .map(|item| item.frontier_id.clone())
        .chain(
            frontier
                .unresolved_keys
                .iter()
                .map(|key| key.frontier_id.clone()),
        )
        .collect::<Vec<_>>();
    frontier_ids.sort();
    frontier_ids.dedup();

    for attempt in 0..SESSION_FRONTIER_CLAIMS_MAX_ATTEMPTS {
        let now = unix_time_secs()?;
        let expires_at_unix_secs = now.checked_add(request.ttl_secs).ok_or_else(|| {
            explain_refusal(
                "Frontier claim TTL is too large",
                json!({"ttl_secs": request.ttl_secs, "writes_performed": false}),
                "Pass a smaller --ttl-secs value",
            )
        })?;
        let current = read_claim_log(&session)?;
        let active = active_claims(&current.events, now);
        let claimed_elsewhere = active
            .values()
            .filter(|claim| claim.solve_hash == solve_hash)
            .flat_map(|claim| claim.frontier_ids.iter().cloned())
            .collect::<BTreeSet<_>>();
        let selected = frontier_ids
            .iter()
            .filter(|frontier_id| !claimed_elsewhere.contains(*frontier_id))
            .take(request.limit)
            .cloned()
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Ok(SessionFrontierClaimOutput {
                version: SESSION_FRONTIER_CLAIMS_VERSION.to_string(),
                solve_hash: solve_hash.clone(),
                claim_id: None,
                worker: request.worker,
                expires_at_unix_secs: None,
                frontier_ids: Vec::new(),
                work_items: Vec::new(),
                unresolved_keys: Vec::new(),
            });
        }
        let claim_material = serde_json::to_vec(&(
            "canon.entity.session.frontier.claim.v1",
            &request.worker,
            &solve_hash,
            &selected,
            expires_at_unix_secs,
            current.generation_id.as_deref(),
        ))
        .expect("claim identity fields serialize");
        let claim_id = format!("frontier-claim:{}", blake3::hash(&claim_material).to_hex());
        let event = FrontierClaimEvent::Claim {
            claim_id: claim_id.clone(),
            worker: request.worker.clone(),
            solve_hash: solve_hash.clone(),
            frontier_ids: selected.clone(),
            expires_at_unix_secs,
        };
        match append_claim_event(&session, &current, &event) {
            Ok(()) => {
                let selected = selected.into_iter().collect::<BTreeSet<_>>();
                return Ok(SessionFrontierClaimOutput {
                    version: SESSION_FRONTIER_CLAIMS_VERSION.to_string(),
                    solve_hash,
                    claim_id: Some(claim_id),
                    worker: request.worker,
                    expires_at_unix_secs: Some(expires_at_unix_secs),
                    frontier_ids: selected.iter().cloned().collect(),
                    work_items: frontier
                        .work_items
                        .into_iter()
                        .filter(|item| selected.contains(&item.frontier_id))
                        .collect(),
                    unresolved_keys: frontier
                        .unresolved_keys
                        .into_iter()
                        .filter(|key| selected.contains(&key.frontier_id))
                        .collect(),
                });
            }
            Err(error) if error.is_stream_head_conflict() => {
                if attempt + 1 < SESSION_FRONTIER_CLAIMS_MAX_ATTEMPTS {
                    thread::sleep(Duration::from_millis(((attempt + 1) * 2) as u64));
                }
            }
            Err(error) => {
                return Err(claim_publication_refusal(
                    error.message,
                    error.kind,
                    error.writes_performed,
                ));
            }
        }
    }

    Err(explain_refusal(
        "Frontier claim could not advance the session lease stream after bounded retries",
        json!({"stream": SESSION_FRONTIER_CLAIMS_STREAM_ID, "writes_performed": true}),
        "Retry the frontier claim after the concurrent claim writers finish",
    ))
}

pub fn release_frontier_claim(
    request: SessionFrontierReleaseRequest,
) -> Result<SessionFrontierReleaseOutput, Refusal> {
    let session = absolute(&request.session)?;
    open_stream(&session, SESSION_MANIFEST_STREAM_ID)?;
    for attempt in 0..SESSION_FRONTIER_CLAIMS_MAX_ATTEMPTS {
        let now = unix_time_secs()?;
        let current = read_claim_log(&session)?;
        let active = active_claims(&current.events, now);
        if !active.contains_key(&request.claim_id) {
            return Ok(SessionFrontierReleaseOutput {
                version: SESSION_FRONTIER_CLAIMS_VERSION.to_string(),
                claim_id: request.claim_id,
                released: false,
            });
        }
        let event = FrontierClaimEvent::Release {
            claim_id: request.claim_id.clone(),
            released_at_unix_secs: now,
        };
        match append_claim_event(&session, &current, &event) {
            Ok(()) => {
                return Ok(SessionFrontierReleaseOutput {
                    version: SESSION_FRONTIER_CLAIMS_VERSION.to_string(),
                    claim_id: request.claim_id,
                    released: true,
                });
            }
            Err(error) if error.is_stream_head_conflict() => {
                if attempt + 1 < SESSION_FRONTIER_CLAIMS_MAX_ATTEMPTS {
                    thread::sleep(Duration::from_millis(((attempt + 1) * 2) as u64));
                }
            }
            Err(error) => {
                return Err(claim_publication_refusal(
                    error.message,
                    error.kind,
                    error.writes_performed,
                ));
            }
        }
    }
    Err(explain_refusal(
        "Frontier claim release could not advance the session lease stream after bounded retries",
        json!({"stream": SESSION_FRONTIER_CLAIMS_STREAM_ID, "writes_performed": true}),
        "Retry the release after the concurrent claim writers finish",
    ))
}

#[derive(Debug)]
struct ClaimLog {
    generation_id: Option<String>,
    events: Vec<FrontierClaimEvent>,
    next_sequence: u64,
}

#[derive(Debug)]
struct ActiveClaim {
    solve_hash: String,
    frontier_ids: Vec<String>,
    expires_at_unix_secs: u64,
}

fn read_claim_log(session: &Path) -> Result<ClaimLog, Refusal> {
    let snapshot = match open_current_stream_generation(session, SESSION_FRONTIER_CLAIMS_STREAM_ID)
    {
        Ok(snapshot) => Some(snapshot),
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => None,
        Err(error) => {
            return Err(claim_publication_refusal(
                error.message,
                error.kind,
                error.writes_performed,
            ));
        }
    };
    let generation_id = snapshot
        .as_ref()
        .map(|snapshot| snapshot.generation_id.clone());
    let mut events = Vec::new();
    let mut next_sequence = 0u64;
    if let Some(snapshot) = snapshot.as_ref() {
        let records_prefix = format!("{SESSION_FRONTIER_CLAIMS_RECORDS_PATH}/");
        for logical_path in snapshot
            .logical_paths()
            .filter(|path| path.starts_with(records_prefix.as_str()))
        {
            let sequence = logical_path
                .strip_prefix(records_prefix.as_str())
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(|sequence| sequence.parse::<u64>().ok())
                .ok_or_else(|| {
                    explain_refusal(
                        format!("Frontier claim record path is invalid: {logical_path}"),
                        json!({"stream": SESSION_FRONTIER_CLAIMS_STREAM_ID, "writes_performed": false}),
                        "Repair the session frontier claim stream before claiming more work",
                    )
                })?;
            if sequence != next_sequence {
                return Err(explain_refusal(
                    format!(
                        "Frontier claim stream sequence expected {next_sequence}, found {sequence}"
                    ),
                    json!({"stream": SESSION_FRONTIER_CLAIMS_STREAM_ID, "writes_performed": false}),
                    "Repair the session frontier claim stream before claiming more work",
                ));
            }
            let bytes = snapshot.read_logical_file(logical_path).ok_or_else(|| {
                explain_refusal(
                    format!("Frontier claim record {sequence} has no stored bytes"),
                    json!({
                        "stream": SESSION_FRONTIER_CLAIMS_STREAM_ID,
                        "logical_path": logical_path,
                        "writes_performed": false,
                    }),
                    "Repair the session frontier claim stream before claiming more work",
                )
            })?;
            let event = serde_json::from_slice(bytes).map_err(|error| {
                explain_refusal(
                    format!("Frontier claim record {sequence} is invalid: {error}"),
                    json!({"stream": SESSION_FRONTIER_CLAIMS_STREAM_ID, "sequence": sequence, "writes_performed": false}),
                    "Repair the session frontier claim log before claiming more work",
                )
            })?;
            events.push(event);
            next_sequence = next_sequence.checked_add(1).ok_or_else(|| {
                explain_refusal(
                    "Frontier claim stream sequence is exhausted",
                    json!({"stream": SESSION_FRONTIER_CLAIMS_STREAM_ID, "writes_performed": false}),
                    "Start a new session before adding more frontier claim records",
                )
            })?;
        }
    }
    Ok(ClaimLog {
        generation_id,
        events,
        next_sequence,
    })
}

fn active_claims(events: &[FrontierClaimEvent], now: u64) -> BTreeMap<String, ActiveClaim> {
    let mut claims = BTreeMap::new();
    for event in events {
        match event {
            FrontierClaimEvent::Claim {
                claim_id,
                solve_hash,
                frontier_ids,
                expires_at_unix_secs,
                ..
            } => {
                claims.insert(
                    claim_id.clone(),
                    ActiveClaim {
                        solve_hash: solve_hash.clone(),
                        frontier_ids: frontier_ids.clone(),
                        expires_at_unix_secs: *expires_at_unix_secs,
                    },
                );
            }
            FrontierClaimEvent::Release { claim_id, .. } => {
                claims.remove(claim_id);
            }
        }
    }
    claims.retain(|_, claim: &mut ActiveClaim| claim.expires_at_unix_secs > now);
    claims
}

fn append_claim_event(
    session: &Path,
    current: &ClaimLog,
    event: &FrontierClaimEvent,
) -> Result<(), crate::entity::publication::EntityPublicationError> {
    let bytes = serde_json::to_vec(event).expect("claim events serialize");
    let logical_path = format!(
        "{SESSION_FRONTIER_CLAIMS_RECORDS_PATH}/{:020}.json",
        current.next_sequence
    );
    let upstream_artifacts = match event {
        FrontierClaimEvent::Claim { solve_hash, .. } => vec![EntityPublicationUpstreamRef {
            version: SESSION_SOLVE_STREAM_ID.to_string(),
            content_hash: solve_hash.clone(),
        }],
        FrontierClaimEvent::Release { .. } => Vec::new(),
    };
    let request = EntityPublicationRequest {
        stream_id: SESSION_FRONTIER_CLAIMS_STREAM_ID.to_string(),
        supersedes_generation_id: current.generation_id.clone(),
        request_fingerprint: blake3::hash(SESSION_FRONTIER_CLAIMS_VERSION.as_bytes())
            .to_hex()
            .to_string(),
        cache_mode: "disabled".to_string(),
        cache_status: "cold_start".to_string(),
        cache_receipt_hash: blake3::hash(b"canon.entity.session.frontier.claims.receipt.v1")
            .to_hex()
            .to_string(),
        stage_order: vec!["session".to_string()],
        upstream_artifacts,
        files: vec![EntityPublicationFileInput::new(
            logical_path,
            "session",
            SESSION_FRONTIER_CLAIMS_VERSION,
            bytes,
        )],
        omit_logical_paths: Vec::new(),
    };
    publish_stream_patch(session, request).map(|_| ())
}

fn unix_time_secs() -> Result<u64, Refusal> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| {
            explain_refusal(
                format!("System clock is before the Unix epoch: {error}"),
                json!({"writes_performed": false}),
                "Correct the system clock and retry the frontier lease operation",
            )
        })
}

fn claim_publication_refusal(
    message: String,
    kind: EntityPublicationErrorKind,
    writes_performed: bool,
) -> Refusal {
    explain_refusal(
        format!("Unable to update the session frontier claim stream: {message}"),
        json!({
            "stream": SESSION_FRONTIER_CLAIMS_STREAM_ID,
            "kind": format!("{kind:?}"),
            "writes_performed": writes_performed,
        }),
        "Repair the session publication and retry the frontier lease operation",
    )
}

fn absolute(path: &Path) -> Result<PathBuf, Refusal> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|error| {
            explain_refusal(
                format!("Unable to resolve session path: {error}"),
                json!({"writes_performed": false}),
                "Pass an absolute session path",
            )
        })
}

/// One thing the frontier counts: a review pair, a contradiction or an
/// oversized cluster.
#[derive(Debug, Clone)]
struct Item {
    group_id: &'static str,
    surfaces: Vec<String>,
    settles_with: Vec<String>,
    identity_context: Vec<String>,
    /// Present for review pairs only.
    review: Option<ReviewFacts>,
    cannot_link_vetoes: Vec<FrontierCannotLinkVeto>,
}

#[derive(Debug, Clone)]
struct ReviewFacts {
    evidence_types: Vec<String>,
}

fn group_for_missing(missing: &str) -> &'static str {
    match missing {
        "second_lineage" => NEEDS_INDEPENDENT_ORIGIN,
        "second_evidence_type" => NEEDS_SECOND_EVIDENCE_TYPE,
        "required_evidence_type" => NEEDS_AUTHORITATIVE_KEY,
        _ => NEEDS_HUMAN,
    }
}

fn items_of(solve: &SessionSolveArtifact) -> Vec<Item> {
    let mut items = Vec::new();
    for review in &solve.review {
        let group_id = group_for_missing(&review.missing);
        let settles_with = match group_id {
            NEEDS_AUTHORITATIVE_KEY => solve.authoritative_keys.clone(),
            NEEDS_SECOND_EVIDENCE_TYPE => TIER_B_TYPES
                .iter()
                .filter(|kind| !review.evidence_types.iter().any(|seen| seen == **kind))
                .map(|kind| (*kind).to_string())
                .collect(),
            NEEDS_INDEPENDENT_ORIGIN => TIER_B_TYPES
                .iter()
                .map(|kind| (*kind).to_string())
                .collect(),
            _ => Vec::new(),
        };
        items.push(Item {
            group_id,
            surfaces: vec![review.left_surface.clone(), review.right_surface.clone()],
            settles_with,
            identity_context: Vec::new(),
            review: Some(ReviewFacts {
                evidence_types: review.evidence_types.clone(),
            }),
            cannot_link_vetoes: Vec::new(),
        });
    }
    for contradiction in &solve.contradictions {
        items.push(Item {
            group_id: NEEDS_HUMAN,
            surfaces: contradiction.surfaces.clone(),
            settles_with: Vec::new(),
            identity_context: std::iter::once(format!("reason:{}", contradiction.reason))
                .chain(
                    contradiction
                        .obs_ids
                        .iter()
                        .map(|id| format!("observation:{id}")),
                )
                .chain(
                    contradiction
                        .evidence_ids
                        .iter()
                        .map(|id| format!("evidence:{id}")),
                )
                .collect(),
            review: None,
            cannot_link_vetoes: if matches!(
                contradiction.reason.as_str(),
                "distinct" | "registry_cannot_link"
            ) {
                vec![FrontierCannotLinkVeto {
                    reason: contradiction.reason.clone(),
                    evidence_ids: contradiction.evidence_ids.clone(),
                }]
            } else {
                Vec::new()
            },
        });
    }
    for cluster in solve
        .clusters
        .iter()
        .filter(|cluster| cluster.outcome == "oversized_cluster")
    {
        items.push(Item {
            group_id: NEEDS_HUMAN,
            surfaces: cluster.surfaces.clone(),
            settles_with: Vec::new(),
            identity_context: vec![format!("oversized_cluster:{}", cluster.cluster_id)],
            review: None,
            cannot_link_vetoes: Vec::new(),
        });
    }
    items
}

pub(crate) fn build_frontier(
    solve: &SessionSolveArtifact,
    solve_hash: &str,
    observations: &[ObservationView],
    seed_entity: Option<&str>,
    shard: Option<FrontierShard>,
    limit: Option<usize>,
) -> Result<SessionFrontierOutput, Refusal> {
    let seed = match seed_entity {
        Some(query) => Some(resolve_seed(solve, observations, query)?),
        None => None,
    };
    let scope = seed.as_ref().map(|seed| &seed.neighbourhood);
    let in_scope = |surfaces: &[String]| {
        scope.is_none_or(|scope| surfaces.iter().any(|surface| scope.contains(surface)))
    };

    let by_surface = surfaces_to_observations(observations);
    let mut items = items_of(solve)
        .into_iter()
        .filter(|item| in_scope(&item.surfaces))
        .collect::<Vec<_>>();

    let mut unresolved_keys = unresolved_keys_of(solve, observations, scope);
    let mut candidate_ids = items
        .iter()
        .map(item_frontier_id)
        .chain(unresolved_keys.iter().map(|key| key.frontier_id.clone()))
        .filter(|id| shard.is_none_or(|shard| id_in_shard(id, shard)))
        .collect::<Vec<_>>();
    candidate_ids.sort();
    candidate_ids.dedup();
    if let Some(limit) = limit {
        candidate_ids.truncate(limit);
    }
    let selected = candidate_ids.into_iter().collect::<BTreeSet<_>>();
    items.retain(|item| selected.contains(&item_frontier_id(item)));
    unresolved_keys.retain(|key| selected.contains(&key.frontier_id));

    let components = components_of(&items, &by_surface);
    let mut work_items = work_items_of(solve, &items, &by_surface);
    work_items.extend(key_work_items_of(solve, &unresolved_keys, &by_surface));
    work_items.sort_by(|left, right| left.frontier_id.cmp(&right.frontier_id));
    let history_requests = history_requests_of(&items, &by_surface);
    let coverage = coverage_of(solve, observations, scope);
    let terminal = terminal_of(solve, scope);

    let seed_block = seed.map(|seed| {
        seed_block_of(
            solve,
            observations,
            &items,
            &history_requests,
            seed,
            seed_entity.unwrap_or_default(),
        )
    });
    Ok(SessionFrontierOutput {
        version: SESSION_FRONTIER_VERSION.to_string(),
        solve_hash: solve_hash.to_string(),
        coverage,
        unresolved_keys,
        components,
        work_items,
        terminal,
        history_requests,
        seed: seed_block,
        partition: shard.map(|shard| FrontierPartition {
            index: shard.index,
            total: shard.total,
            limit,
        }),
    })
}

const FRONTIER_MAX_OBSERVATIONS_PER_ITEM: usize = 32;
const FRONTIER_MAX_LINEAGES_PER_OBSERVATION: usize = 16;
const FRONTIER_MAX_KEY_FAMILIES_PER_OBSERVATION: usize = 32;
const FRONTIER_MAX_KEY_VALUES_PER_FAMILY: usize = 16;

fn item_frontier_id(item: &Item) -> String {
    let mut surfaces = item.surfaces.clone();
    surfaces.sort();
    surfaces.dedup();
    let mut settles_with = item.settles_with.clone();
    settles_with.sort();
    settles_with.dedup();
    let mut evidence_types = item
        .review
        .as_ref()
        .map(|review| review.evidence_types.clone())
        .unwrap_or_default();
    evidence_types.sort();
    evidence_types.dedup();
    let mut vetoes = item.cannot_link_vetoes.clone();
    vetoes.sort();
    vetoes.dedup();
    let mut identity_context = item.identity_context.clone();
    identity_context.sort();
    identity_context.dedup();
    let material = serde_json::to_vec(&(
        "canon.entity.session.frontier.item.v1",
        item.group_id,
        surfaces,
        settles_with,
        evidence_types,
        vetoes,
        identity_context,
    ))
    .expect("frontier identity fields serialize");
    format!("frontier:{}", blake3::hash(&material).to_hex())
}

fn id_in_shard(id: &str, shard: FrontierShard) -> bool {
    let Some(digest) = id.strip_prefix("frontier:") else {
        return false;
    };
    let Some(prefix_hex) = digest.get(..16) else {
        return false;
    };
    let Ok(prefix) = u64::from_str_radix(prefix_hex, 16) else {
        return false;
    };
    prefix % shard.total == shard.index
}

fn work_items_of(
    solve: &SessionSolveArtifact,
    items: &[Item],
    by_surface: &BTreeMap<&str, Vec<&ObservationView>>,
) -> Vec<FrontierWorkItem> {
    let mut work_items = items
        .iter()
        .map(|item| {
            let mut surfaces = item.surfaces.clone();
            surfaces.sort();
            surfaces.dedup();
            let (observations, observations_truncated) =
                observations_for_surfaces(&surfaces, by_surface);
            let mut incumbent_candidates = solve
                .clusters
                .iter()
                .filter(|cluster| {
                    cluster
                        .surfaces
                        .iter()
                        .any(|surface| surfaces.binary_search(surface).is_ok())
                })
                .filter_map(|cluster| cluster.incumbent_canonical_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            incumbent_candidates.sort();
            let existing_partial_evidence = item
                .review
                .as_ref()
                .map(|review| review.evidence_types.clone())
                .unwrap_or_default();
            let mut needed_evidence_kinds = item.settles_with.clone();
            needed_evidence_kinds.sort();
            needed_evidence_kinds.dedup();
            FrontierWorkItem {
                frontier_id: item_frontier_id(item),
                missing: item.group_id.to_string(),
                surfaces,
                observations,
                observations_truncated,
                incumbent_candidates,
                existing_partial_evidence,
                cannot_link_vetoes: item.cannot_link_vetoes.clone(),
                needed_evidence_kinds,
            }
        })
        .collect::<Vec<_>>();
    work_items.sort_by(|left, right| left.frontier_id.cmp(&right.frontier_id));
    work_items
}

fn key_work_items_of(
    solve: &SessionSolveArtifact,
    keys: &[FrontierUnresolvedKey],
    by_surface: &BTreeMap<&str, Vec<&ObservationView>>,
) -> Vec<FrontierWorkItem> {
    keys.iter()
        .map(|key| {
            let (observations, observations_truncated) =
                observations_for_surfaces(&key.surfaces, by_surface);
            let incumbent_candidates = solve
                .clusters
                .iter()
                .filter(|cluster| {
                    cluster
                        .surfaces
                        .iter()
                        .any(|surface| key.surfaces.contains(surface))
                })
                .filter_map(|cluster| cluster.incumbent_canonical_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let cannot_link_vetoes = solve
                .contradictions
                .iter()
                .filter(|contradiction| {
                    matches!(
                        contradiction.reason.as_str(),
                        "distinct" | "registry_cannot_link"
                    ) && contradiction
                        .surfaces
                        .iter()
                        .any(|surface| key.surfaces.contains(surface))
                })
                .map(|contradiction| FrontierCannotLinkVeto {
                    reason: contradiction.reason.clone(),
                    evidence_ids: contradiction.evidence_ids.clone(),
                })
                .collect();
            FrontierWorkItem {
                frontier_id: key.frontier_id.clone(),
                missing: NEEDS_AUTHORITATIVE_KEY.to_string(),
                surfaces: key.surfaces.clone(),
                observations,
                observations_truncated,
                incumbent_candidates,
                existing_partial_evidence: Vec::new(),
                cannot_link_vetoes,
                needed_evidence_kinds: vec![key.namespace.clone()],
            }
        })
        .collect()
}

fn observations_for_surfaces(
    surfaces: &[String],
    by_surface: &BTreeMap<&str, Vec<&ObservationView>>,
) -> (Vec<FrontierObservation>, bool) {
    let mut details_truncated = false;
    let mut observations = surfaces
        .iter()
        .flat_map(|surface| by_surface.get(surface.as_str()).into_iter().flatten())
        .map(|observation| {
            let lineages = observation
                .lineages
                .iter()
                .take(FRONTIER_MAX_LINEAGES_PER_OBSERVATION)
                .cloned()
                .collect::<Vec<_>>();
            let mut truncated = observation.lineages.len() > lineages.len();
            let keys = observation
                .keys
                .iter()
                .take(FRONTIER_MAX_KEY_FAMILIES_PER_OBSERVATION)
                .map(|(namespace, observed_values)| {
                    let values = observed_values
                        .iter()
                        .take(FRONTIER_MAX_KEY_VALUES_PER_FAMILY)
                        .cloned()
                        .collect::<Vec<_>>();
                    truncated |= observed_values.len() > values.len();
                    (namespace.clone(), values)
                })
                .collect::<BTreeMap<_, _>>();
            truncated |= observation.keys.len() > keys.len();
            details_truncated |= truncated;
            FrontierObservation {
                obs_id: observation.obs_id.clone(),
                surface: observation.surface.clone(),
                lineages,
                keys,
                details_truncated: truncated,
            }
        })
        .collect::<Vec<_>>();
    observations
        .sort_by(|left, right| (&left.surface, &left.obs_id).cmp(&(&right.surface, &right.obs_id)));
    observations
        .dedup_by(|left, right| left.surface == right.surface && left.obs_id == right.obs_id);
    let truncated = observations.len() > FRONTIER_MAX_OBSERVATIONS_PER_ITEM || details_truncated;
    observations.truncate(FRONTIER_MAX_OBSERVATIONS_PER_ITEM);
    (observations, truncated)
}

fn surfaces_to_observations(
    observations: &[ObservationView],
) -> BTreeMap<&str, Vec<&ObservationView>> {
    let mut by_surface = BTreeMap::<&str, Vec<&ObservationView>>::new();
    for observation in observations {
        by_surface
            .entry(observation.surface.as_str())
            .or_default()
            .push(observation);
    }
    by_surface
}

fn components_of(
    items: &[Item],
    by_surface: &BTreeMap<&str, Vec<&ObservationView>>,
) -> Vec<FrontierComponent> {
    let mut groups = BTreeMap::<&'static str, Vec<&Item>>::new();
    for item in items {
        groups.entry(item.group_id).or_default().push(item);
    }
    groups
        .into_iter()
        .map(|(group_id, group)| {
            let surfaces = group
                .iter()
                .flat_map(|item| item.surfaces.iter().cloned())
                .collect::<BTreeSet<_>>();
            let settles_with = group
                .iter()
                .flat_map(|item| item.settles_with.iter().cloned())
                .collect::<BTreeSet<_>>();
            let observation_count = surfaces
                .iter()
                .map(|surface| by_surface.get(surface.as_str()).map_or(0, Vec::len))
                .sum();
            FrontierComponent {
                group_id: group_id.to_string(),
                missing: group_id.to_string(),
                settles_with: settles_with.into_iter().collect(),
                items: group.len(),
                surfaces: surfaces.into_iter().collect(),
                observation_count,
            }
        })
        .collect()
}

/// A history request for each review pair that `same_slot` could settle: the
/// context the pair's observations share names the scope, and the earliest
/// as-of date among them is the `before` bound.
fn history_requests_of(
    items: &[Item],
    by_surface: &BTreeMap<&str, Vec<&ObservationView>>,
) -> Vec<FrontierHistoryRequest> {
    let mut requests = BTreeSet::<(String, BTreeMap<String, String>, Option<String>)>::new();
    for item in items {
        let Some(review) = &item.review else {
            continue;
        };
        if review.evidence_types.iter().any(|kind| kind == "same_slot") {
            continue;
        }
        let observations = item
            .surfaces
            .iter()
            .flat_map(|surface| by_surface.get(surface.as_str()).into_iter().flatten())
            .copied()
            .collect::<Vec<_>>();
        let Some(scope) = shared_scope(&observations) else {
            continue;
        };
        let before = observations
            .iter()
            .filter_map(|observation| observation.as_of.clone())
            .min();
        requests.insert((item.group_id.to_string(), scope, before));
    }
    requests
        .into_iter()
        .map(|(group_id, scope, before)| FrontierHistoryRequest {
            group_id,
            need: "prior_observations".to_string(),
            scope,
            before,
            settles_with: vec!["same_slot".to_string()],
        })
        .collect()
}

/// Context entries every observation carries with the same value, leaving out
/// the as-of dates. `None` when nothing is shared.
fn shared_scope(observations: &[&ObservationView]) -> Option<BTreeMap<String, String>> {
    let mut iter = observations.iter();
    let first = iter.next()?;
    let mut scope = first
        .context
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "as_of" | "as_of_date"))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    for observation in iter {
        scope.retain(|name, value| observation.context.get(name) == Some(value));
    }
    (!scope.is_empty()).then_some(scope)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SurfaceState {
    Supported,
    Conflicted,
    Unresolved,
}

fn surface_states(solve: &SessionSolveArtifact) -> BTreeMap<String, SurfaceState> {
    let mut states = BTreeMap::new();
    for cluster in &solve.clusters {
        let state = match cluster.outcome.as_str() {
            "accepted" | "resolved_existing" => SurfaceState::Supported,
            "contradiction" => SurfaceState::Conflicted,
            _ => SurfaceState::Unresolved,
        };
        for surface in &cluster.surfaces {
            states.insert(surface.clone(), state);
        }
    }
    for contradiction in &solve.contradictions {
        for surface in &contradiction.surfaces {
            states.insert(surface.clone(), SurfaceState::Conflicted);
        }
    }
    states
}

fn sources_of(observation: &ObservationView) -> Vec<String> {
    if observation.lineages.is_empty() {
        vec![UNKNOWN_SOURCE.to_string()]
    } else {
        observation.lineages.iter().cloned().collect()
    }
}

fn coverage_of(
    solve: &SessionSolveArtifact,
    observations: &[ObservationView],
    scope: Option<&BTreeSet<String>>,
) -> Vec<FrontierCoverage> {
    let states = surface_states(solve);
    let mut attempted = BTreeMap::<(String, String), BTreeSet<String>>::new();
    for observation in observations {
        if scope.is_some_and(|scope| !scope.contains(&observation.surface)) {
            continue;
        }
        let families = observation
            .keys
            .keys()
            .cloned()
            .chain(std::iter::once("name".to_string()))
            .collect::<BTreeSet<_>>();
        for source in sources_of(observation) {
            for family in &families {
                attempted
                    .entry((family.clone(), source.clone()))
                    .or_default()
                    .insert(observation.surface.clone());
            }
        }
    }
    attempted
        .into_iter()
        .map(|((key_family, source), surfaces)| {
            let count = |wanted: SurfaceState| {
                surfaces
                    .iter()
                    .filter(|surface| {
                        states
                            .get(*surface)
                            .copied()
                            .unwrap_or(SurfaceState::Unresolved)
                            == wanted
                    })
                    .count()
            };
            FrontierCoverage {
                key_family,
                source,
                attempted: surfaces.len(),
                supported: count(SurfaceState::Supported),
                conflicted: count(SurfaceState::Conflicted),
                unresolved: count(SurfaceState::Unresolved),
            }
        })
        .collect()
}

fn has_partner(cluster: Option<&SessionSolveCluster>) -> bool {
    cluster.is_some_and(|cluster| {
        cluster.surfaces.len() > 1 || cluster.incumbent_canonical_id.is_some()
    })
}

fn unresolved_keys_of(
    solve: &SessionSolveArtifact,
    observations: &[ObservationView],
    scope: Option<&BTreeSet<String>>,
) -> Vec<FrontierUnresolvedKey> {
    let cluster_of = |surface: &str| {
        solve
            .clusters
            .iter()
            .find(|cluster| cluster.surfaces.iter().any(|member| member == surface))
    };
    let contradicted = solve
        .contradictions
        .iter()
        .flat_map(|contradiction| contradiction.surfaces.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut values = BTreeMap::<(String, String), (BTreeSet<String>, BTreeSet<String>)>::new();
    for observation in observations {
        for namespace in &solve.authoritative_keys {
            for value in observation.keys.get(namespace).into_iter().flatten() {
                let entry = values
                    .entry((namespace.clone(), value.clone()))
                    .or_default();
                entry.0.insert(observation.surface.clone());
                entry.1.extend(sources_of(observation));
            }
        }
    }
    values
        .into_iter()
        .filter(|(_, (surfaces, _))| {
            let alone = surfaces.len() == 1
                && surfaces
                    .iter()
                    .all(|surface| !has_partner(cluster_of(surface)));
            let conflicting = surfaces
                .iter()
                .any(|surface| contradicted.contains(surface));
            (alone || conflicting)
                && scope.is_none_or(|scope| surfaces.iter().any(|surface| scope.contains(surface)))
        })
        .map(|((namespace, value), (surfaces, sources))| {
            let material =
                serde_json::to_vec(&("canon.entity.session.frontier.key.v1", &namespace, &value))
                    .expect("key serializes");
            FrontierUnresolvedKey {
                frontier_id: format!("frontier:{}", blake3::hash(&material).to_hex()),
                namespace,
                value,
                surfaces: surfaces.into_iter().collect(),
                sources: sources.into_iter().collect(),
            }
        })
        .collect()
}

fn terminal_of(
    solve: &SessionSolveArtifact,
    scope: Option<&BTreeSet<String>>,
) -> Vec<FrontierTerminal> {
    let in_review_or_contradiction = solve
        .review
        .iter()
        .flat_map(|review| [review.left_surface.clone(), review.right_surface.clone()])
        .chain(
            solve
                .contradictions
                .iter()
                .flat_map(|contradiction| contradiction.surfaces.iter().cloned()),
        )
        .collect::<BTreeSet<_>>();
    let standalone = solve
        .clusters
        .iter()
        .filter(|cluster| cluster.outcome == "standalone")
        .filter(|cluster| {
            cluster
                .surfaces
                .iter()
                .all(|surface| !in_review_or_contradiction.contains(surface))
        })
        .filter(|cluster| {
            scope.is_none_or(|scope| {
                cluster
                    .surfaces
                    .iter()
                    .any(|surface| scope.contains(surface))
            })
        })
        .count();
    let mut rows = Vec::new();
    if scope.is_none() && solve.dangling_evidence_count > 0 {
        rows.push(FrontierTerminal {
            reason: "dangling_evidence".to_string(),
            count: solve.dangling_evidence_count,
        });
    }
    if standalone > 0 {
        rows.push(FrontierTerminal {
            reason: "standalone_no_positive_evidence".to_string(),
            count: standalone,
        });
    }
    rows.sort_by(|left, right| left.reason.cmp(&right.reason));
    rows
}

struct ResolvedSeed {
    canonical_id: Option<String>,
    cluster_id: Option<String>,
    cluster_surfaces: BTreeSet<String>,
    linked: BTreeSet<String>,
    units: BTreeSet<String>,
    /// Surfaces linked by `part_of` evidence or detected as units.
    neighbourhood: BTreeSet<String>,
    part_of_linked: BTreeSet<String>,
}

fn resolve_seed(
    solve: &SessionSolveArtifact,
    observations: &[ObservationView],
    query: &str,
) -> Result<ResolvedSeed, Refusal> {
    let clusters = solve
        .clusters
        .iter()
        .filter(|cluster| {
            cluster.incumbent_canonical_id.as_deref() == Some(query)
                || cluster.surfaces.iter().any(|surface| surface == query)
        })
        .collect::<Vec<_>>();
    let mut cluster_surfaces = clusters
        .iter()
        .flat_map(|cluster| cluster.surfaces.iter().cloned())
        .collect::<BTreeSet<_>>();
    if cluster_surfaces.is_empty()
        && observations
            .iter()
            .any(|observation| observation.surface == query)
    {
        cluster_surfaces.insert(query.to_string());
    }
    if cluster_surfaces.is_empty() {
        return Err(explain_refusal(
            format!("Frontier seed entity '{query}' was not found"),
            json!({"seed_entity": query, "writes_performed": false}),
            "Run canon entity session status for the session state, then rerun frontier",
        ));
    }
    let mut linked = BTreeSet::new();
    for relation in solve.relations.iter().filter(|r| r.relation == "part_of") {
        let ends = [
            &relation.left_surface,
            &relation.right_surface,
            &relation.subject_surface,
            &relation.object_surface,
        ];
        if ends.iter().any(|end| cluster_surfaces.contains(*end)) {
            linked.extend(
                ends.iter()
                    .filter(|end| !end.is_empty() && !cluster_surfaces.contains(**end))
                    .map(|end| (*end).clone()),
            );
        }
    }
    let incumbent = clusters
        .iter()
        .find_map(|cluster| cluster.incumbent_canonical_id.clone());
    // A unit looks like one when its observations carry a role and either name
    // the seed in their context or start with the seed's surface.
    let mut units = BTreeSet::new();
    for observation in observations {
        if cluster_surfaces.contains(&observation.surface)
            || linked.contains(&observation.surface)
            || !observation.context.contains_key("role")
        {
            continue;
        }
        let names_seed = observation.context.values().any(|value| {
            cluster_surfaces.contains(value)
                || value == query
                || incumbent.as_deref() == Some(value.as_str())
        });
        let prefixed = cluster_surfaces.iter().any(|surface| {
            observation
                .surface
                .strip_prefix(surface.as_str())
                .is_some_and(|rest| rest.starts_with(' '))
        });
        if names_seed || prefixed {
            units.insert(observation.surface.clone());
        }
    }
    let neighbourhood = cluster_surfaces
        .iter()
        .chain(linked.iter())
        .chain(units.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    Ok(ResolvedSeed {
        canonical_id: incumbent,
        cluster_id: clusters.first().map(|cluster| cluster.cluster_id.clone()),
        cluster_surfaces,
        part_of_linked: linked.clone(),
        linked,
        units,
        neighbourhood,
    })
}

fn seed_block_of(
    solve: &SessionSolveArtifact,
    observations: &[ObservationView],
    items: &[Item],
    history_requests: &[FrontierHistoryRequest],
    seed: ResolvedSeed,
    query: &str,
) -> FrontierSeed {
    let keys_of = |surfaces: &BTreeSet<String>| {
        let mut present = BTreeSet::new();
        for observation in observations
            .iter()
            .filter(|observation| surfaces.contains(&observation.surface))
        {
            present.extend(observation.keys.keys().cloned());
        }
        for cluster in solve.clusters.iter().filter(|cluster| {
            cluster
                .surfaces
                .iter()
                .any(|surface| surfaces.contains(surface))
        }) {
            present.extend(cluster.keys.keys().cloned());
        }
        present
    };
    let mut entities = vec![(query.to_string(), seed.cluster_surfaces.clone())];
    for surface in seed.linked.iter().chain(seed.units.iter()) {
        entities.push((surface.clone(), BTreeSet::from([surface.clone()])));
    }
    let mut unresolved_keys = Vec::new();
    for (entity, surfaces) in &entities {
        let present = keys_of(surfaces);
        for namespace in &solve.authoritative_keys {
            if !present.contains(namespace) {
                unresolved_keys.push(FrontierSeedKey {
                    entity: entity.clone(),
                    namespace: namespace.clone(),
                    settles_with: vec!["same_key".to_string()],
                });
            }
        }
    }
    let mut spellings = BTreeMap::<String, (String, Vec<String>)>::new();
    for item in items.iter().filter(|item| item.review.is_some()) {
        for surface in &item.surfaces {
            spellings
                .entry(surface.clone())
                .or_insert_with(|| (item.group_id.to_string(), item.settles_with.clone()));
        }
    }
    let unresolved_spellings = spellings
        .into_iter()
        .map(|(surface, (group_id, settles_with))| FrontierSeedSpelling {
            surface,
            group_id,
            settles_with,
        })
        .collect();
    let unresolved_units = seed
        .units
        .iter()
        .filter(|surface| !seed.part_of_linked.contains(*surface))
        .map(|surface| FrontierSeedUnit {
            surface: surface.clone(),
            settles_with: vec!["part_of".to_string()],
        })
        .collect();
    FrontierSeed {
        query: query.to_string(),
        canonical_id: seed.canonical_id,
        cluster_id: seed.cluster_id,
        neighbourhood: seed.neighbourhood.into_iter().collect(),
        unresolved_keys,
        unresolved_spellings,
        unresolved_units,
        history_requests: history_requests.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::session::{
        SessionSolveContradiction, SessionSolveInputs, SessionSolveRegistry, SessionSolveReview,
    };

    fn solve(
        review: Vec<SessionSolveReview>,
        contradictions: Vec<SessionSolveContradiction>,
        clusters: Vec<SessionSolveCluster>,
    ) -> SessionSolveArtifact {
        SessionSolveArtifact {
            version: "v1".to_string(),
            inputs: SessionSolveInputs {
                observations_generation: "o".to_string(),
                evidence_generation: None,
                rules_hash: "r".to_string(),
                registry: SessionSolveRegistry {
                    id: "registry".to_string(),
                    version: "0.1.0".to_string(),
                    snapshot_hash: "s".to_string(),
                    escrow_snapshot_hash: String::new(),
                },
            },
            authoritative_keys: vec!["cik".to_string()],
            commit_key_namespaces: Vec::new(),
            summary: Default::default(),
            clusters,
            review,
            relations: Vec::new(),
            contradictions,
            standalone_count: 0,
            dangling_evidence_count: 0,
            derived_evidence: Vec::new(),
            scoped_aliases: Vec::new(),
            units: Vec::new(),
            shared_names: Vec::new(),
        }
    }

    fn review(left: &str, right: &str, types: &[&str], missing: &str) -> SessionSolveReview {
        SessionSolveReview {
            left_surface: left.to_string(),
            right_surface: right.to_string(),
            evidence_types: types.iter().map(|kind| (*kind).to_string()).collect(),
            lineage_count: 1,
            missing: missing.to_string(),
            known_pending: false,
            escrow_id: None,
        }
    }

    fn cluster(id: &str, outcome: &str, surfaces: &[&str]) -> SessionSolveCluster {
        SessionSolveCluster {
            cluster_id: id.to_string(),
            outcome: outcome.to_string(),
            tier: None,
            surfaces: surfaces
                .iter()
                .map(|surface| (*surface).to_string())
                .collect(),
            observation_count: surfaces.len(),
            keys: BTreeMap::new(),
            incumbent_canonical_id: None,
            accepting_evidence_ids: Vec::new(),
            reason: String::new(),
            conflict_reasons: Vec::new(),
        }
    }

    fn observation(id: &str, surface: &str, lineage: &str, cik: Option<&str>) -> ObservationView {
        ObservationView {
            obs_id: id.to_string(),
            surface: surface.to_string(),
            lineages: BTreeSet::from([lineage.to_string()]),
            keys: cik
                .map(|value| {
                    BTreeMap::from([("cik".to_string(), BTreeSet::from([value.to_string()]))])
                })
                .unwrap_or_default(),
            context: BTreeMap::new(),
            as_of: None,
        }
    }

    #[test]
    fn groups_split_by_missing_condition_with_their_settles_with() {
        let mut reviews = Vec::new();
        for index in 0..20 {
            reviews.push(review(
                &format!("o{index}a"),
                &format!("o{index}b"),
                &["name_equivalent", "same_slot"],
                "second_lineage",
            ));
        }
        for index in 0..7 {
            reviews.push(review(
                &format!("t{index}a"),
                &format!("t{index}b"),
                &["name_equivalent"],
                "second_evidence_type",
            ));
        }
        let contradictions = (0..3)
            .map(|index| SessionSolveContradiction {
                reason: "distinct".to_string(),
                surfaces: vec![format!("c{index}a"), format!("c{index}b")],
                obs_ids: Vec::new(),
                evidence_ids: Vec::new(),
            })
            .collect();
        let artifact = solve(reviews, contradictions, Vec::new());
        let frontier = build_frontier(&artifact, "h", &[], None, None, None).unwrap();
        let groups = frontier
            .components
            .iter()
            .map(|group| (group.group_id.as_str(), group))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups["needs_independent_origin"].items, 20);
        assert_eq!(
            groups["needs_independent_origin"].settles_with,
            ["attribute_agreement", "name_equivalent", "same_slot"]
        );
        assert_eq!(groups["needs_second_evidence_type"].items, 7);
        assert_eq!(
            groups["needs_second_evidence_type"].settles_with,
            ["attribute_agreement", "same_slot"],
            "the type already present settles nothing new"
        );
        assert_eq!(groups["needs_human"].items, 3);
        assert!(groups["needs_human"].settles_with.is_empty());
        // Deterministic order: by group id.
        let ids = frontier
            .components
            .iter()
            .map(|group| group.group_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "needs_human",
                "needs_independent_origin",
                "needs_second_evidence_type"
            ]
        );
    }

    #[test]
    fn required_type_gap_names_the_authoritative_namespaces() {
        let artifact = solve(
            vec![review(
                "a",
                "b",
                &["name_equivalent"],
                "required_evidence_type",
            )],
            Vec::new(),
            Vec::new(),
        );
        let frontier = build_frontier(&artifact, "h", &[], None, None, None).unwrap();
        assert_eq!(frontier.components[0].group_id, "needs_authoritative_key");
        assert_eq!(frontier.components[0].settles_with, ["cik"]);
    }

    #[test]
    fn shards_are_a_complete_disjoint_partition_and_tasks_carry_bounded_evidence() {
        let reviews = (0..80)
            .map(|index| {
                review(
                    &format!("pair-{index:03}-left"),
                    &format!("pair-{index:03}-right"),
                    &["name_equivalent"],
                    "second_evidence_type",
                )
            })
            .collect();
        let mut incumbent = cluster("incumbent", "accepted", &["pair-000-left"]);
        incumbent.incumbent_canonical_id = Some("ORG-001".to_string());
        let artifact = solve(
            reviews,
            vec![SessionSolveContradiction {
                reason: "distinct".to_string(),
                surfaces: vec!["veto-left".to_string(), "veto-right".to_string()],
                obs_ids: vec!["veto-proof".to_string()],
                evidence_ids: vec!["evidence-veto".to_string()],
            }],
            vec![incumbent],
        );
        let mut observations = (0..160)
            .map(|index| {
                let pair = index / 2;
                let side = if index % 2 == 0 { "left" } else { "right" };
                observation(
                    &format!("obs-{index:03}"),
                    &format!("pair-{pair:03}-{side}"),
                    "source-a",
                    Some(&format!("key-{index:03}")),
                )
            })
            .collect::<Vec<_>>();
        observations.push(observation("veto-obs", "veto-left", "source-v", None));
        observations.push(observation("veto-obs2", "veto-right", "source-v", None));
        let full = build_frontier(&artifact, "solve", &observations, None, None, None).unwrap();
        assert_eq!(
            full.work_items
                .iter()
                .map(|item| item.frontier_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            240
        );
        let review_task = full
            .work_items
            .iter()
            .find(|item| item.surfaces.contains(&"pair-000-left".to_string()))
            .unwrap();
        assert_eq!(review_task.existing_partial_evidence, ["name_equivalent"]);
        assert_eq!(
            review_task.needed_evidence_kinds,
            ["attribute_agreement", "same_slot"]
        );
        assert_eq!(review_task.incumbent_candidates, ["ORG-001"]);
        assert_eq!(review_task.observations.len(), 2);
        assert_eq!(review_task.observations[0].obs_id, "obs-000");
        let veto = full
            .work_items
            .iter()
            .find(|item| item.surfaces.contains(&"veto-left".to_string()))
            .unwrap();
        assert_eq!(veto.cannot_link_vetoes.len(), 1);
        assert_eq!(veto.cannot_link_vetoes[0].reason, "distinct");
        assert_eq!(veto.cannot_link_vetoes[0].evidence_ids, ["evidence-veto"]);
        let key_task = full
            .work_items
            .iter()
            .find(|item| item.missing == NEEDS_AUTHORITATIVE_KEY)
            .unwrap();
        assert_eq!(key_task.needed_evidence_kinds, ["cik"]);
        assert_eq!(key_task.observations.len(), 1);

        let mut union = BTreeSet::new();
        for index in 0..4 {
            let shard = build_frontier(
                &artifact,
                "solve",
                &observations,
                None,
                Some(FrontierShard { index, total: 4 }),
                None,
            )
            .unwrap();
            let local = shard
                .work_items
                .iter()
                .map(|item| item.frontier_id.clone())
                .chain(
                    shard
                        .unresolved_keys
                        .iter()
                        .map(|key| key.frontier_id.clone()),
                )
                .collect::<BTreeSet<_>>();
            assert!(
                local.into_iter().all(|id| union.insert(id)),
                "shard {index} overlaps"
            );
        }
        let expected = full
            .work_items
            .iter()
            .map(|item| item.frontier_id.clone())
            .chain(
                full.unresolved_keys
                    .iter()
                    .map(|key| key.frontier_id.clone()),
            )
            .collect::<BTreeSet<_>>();
        assert_eq!(union, expected);

        let limited =
            build_frontier(&artifact, "solve", &observations, None, None, Some(7)).unwrap();
        let limited_ids = limited
            .work_items
            .iter()
            .map(|item| item.frontier_id.clone())
            .chain(
                limited
                    .unresolved_keys
                    .iter()
                    .map(|key| key.frontier_id.clone()),
            )
            .collect::<BTreeSet<_>>();
        assert_eq!(limited_ids.len(), 7);
    }

    #[test]
    fn work_item_observations_are_capped_with_an_explicit_truncation_marker() {
        let artifact = solve(
            vec![review(
                "shared-left",
                "shared-right",
                &["same_slot"],
                "second_lineage",
            )],
            Vec::new(),
            Vec::new(),
        );
        let observations = (0..40)
            .map(|index| {
                observation(
                    &format!("obs-{index:02}"),
                    if index % 2 == 0 {
                        "shared-left"
                    } else {
                        "shared-right"
                    },
                    "source",
                    None,
                )
            })
            .collect::<Vec<_>>();
        let frontier = build_frontier(&artifact, "solve", &observations, None, None, None).unwrap();
        assert_eq!(frontier.work_items.len(), 1);
        assert_eq!(
            frontier.work_items[0].observations.len(),
            FRONTIER_MAX_OBSERVATIONS_PER_ITEM
        );
        assert!(frontier.work_items[0].observations_truncated);
    }

    #[test]
    fn shard_syntax_rejects_empty_and_out_of_range_partitions() {
        assert_eq!(
            FrontierShard::parse("0/4"),
            Ok(FrontierShard { index: 0, total: 4 })
        );
        for invalid in ["0", "1/0", "4/4", "x/2", "0/x"] {
            assert!(FrontierShard::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn distinct_contradictions_on_the_same_surfaces_keep_distinct_frontier_ids() {
        let frontier = build_frontier(
            &solve(
                Vec::new(),
                vec![
                    SessionSolveContradiction {
                        reason: "key_conflict".to_string(),
                        surfaces: vec!["same-left".to_string(), "same-right".to_string()],
                        obs_ids: vec!["obs-a".to_string()],
                        evidence_ids: vec!["evidence-a".to_string()],
                    },
                    SessionSolveContradiction {
                        reason: "temporal_conflict".to_string(),
                        surfaces: vec!["same-left".to_string(), "same-right".to_string()],
                        obs_ids: vec!["obs-b".to_string()],
                        evidence_ids: vec!["evidence-b".to_string()],
                    },
                ],
                Vec::new(),
            ),
            "solve",
            &[],
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(frontier.work_items.len(), 2);
        assert_ne!(
            frontier.work_items[0].frontier_id,
            frontier.work_items[1].frontier_id
        );
    }

    #[test]
    fn lease_events_release_items_and_expire_without_mutating_the_log() {
        let events = vec![
            FrontierClaimEvent::Claim {
                claim_id: "live".to_string(),
                worker: "worker-a".to_string(),
                solve_hash: "solve-current".to_string(),
                frontier_ids: vec!["frontier:a".to_string()],
                expires_at_unix_secs: 200,
            },
            FrontierClaimEvent::Claim {
                claim_id: "expired".to_string(),
                worker: "worker-b".to_string(),
                solve_hash: "solve-current".to_string(),
                frontier_ids: vec!["frontier:b".to_string()],
                expires_at_unix_secs: 100,
            },
            FrontierClaimEvent::Claim {
                claim_id: "released".to_string(),
                worker: "worker-c".to_string(),
                solve_hash: "solve-current".to_string(),
                frontier_ids: vec!["frontier:c".to_string()],
                expires_at_unix_secs: 200,
            },
            FrontierClaimEvent::Release {
                claim_id: "released".to_string(),
                released_at_unix_secs: 150,
            },
        ];
        let active = active_claims(&events, 150);
        assert_eq!(active.len(), 1);
        assert_eq!(active["live"].frontier_ids, ["frontier:a"]);
        assert!(!active.contains_key("expired"));
        assert!(!active.contains_key("released"));
        assert_eq!(
            events.len(),
            4,
            "expiry and release do not rewrite prior events"
        );
    }

    #[test]
    fn coverage_counts_distinct_surfaces_per_family_and_source() {
        let artifact = solve(
            Vec::new(),
            vec![SessionSolveContradiction {
                reason: "key_conflict".to_string(),
                surfaces: vec!["Gamma".to_string()],
                obs_ids: Vec::new(),
                evidence_ids: Vec::new(),
            }],
            vec![
                cluster("c1", "accepted", &["Alpha", "Alpha Inc"]),
                cluster("c2", "standalone", &["Beta"]),
                cluster("c3", "contradiction", &["Gamma"]),
            ],
        );
        let observations = vec![
            observation("1", "Alpha", "s1", Some("1")),
            observation("2", "Alpha", "s1", Some("1")),
            observation("3", "Alpha Inc", "s1", Some("1")),
            observation("4", "Beta", "s1", Some("2")),
            observation("5", "Gamma", "s1", Some("3")),
            observation("6", "Beta", "s2", None),
        ];
        let frontier = build_frontier(&artifact, "h", &observations, None, None, None).unwrap();
        let cik = frontier
            .coverage
            .iter()
            .find(|row| row.key_family == "cik" && row.source == "s1")
            .unwrap();
        assert_eq!(
            (cik.attempted, cik.supported, cik.conflicted, cik.unresolved),
            (4, 2, 1, 1),
            "two rows on Alpha are one surface"
        );
        let name = frontier
            .coverage
            .iter()
            .find(|row| row.key_family == "name" && row.source == "s2")
            .unwrap();
        assert_eq!((name.attempted, name.supported, name.unresolved), (1, 0, 1));
        // Beta's key sits on one surface with no partner; Gamma's is in a
        // contradiction; Alpha's key has partners and is not listed.
        let listed = frontier
            .unresolved_keys
            .iter()
            .map(|key| key.value.as_str())
            .collect::<Vec<_>>();
        assert_eq!(listed, ["2", "3"]);
    }

    #[test]
    fn history_request_takes_shared_context_and_the_earliest_date() {
        let artifact = solve(
            vec![review(
                "A",
                "B",
                &["name_equivalent"],
                "second_evidence_type",
            )],
            Vec::new(),
            Vec::new(),
        );
        let mut left = observation("1", "A", "s1", None);
        left.context = BTreeMap::from([
            ("deal".to_string(), "D".to_string()),
            ("loan".to_string(), "L".to_string()),
            ("role".to_string(), "sponsor".to_string()),
            ("as_of_date".to_string(), "2025-03-31".to_string()),
        ]);
        left.as_of = Some("2025-03-31".to_string());
        let mut right = observation("2", "B", "s2", None);
        right.context = BTreeMap::from([
            ("deal".to_string(), "D".to_string()),
            ("loan".to_string(), "L".to_string()),
            ("role".to_string(), "sponsor".to_string()),
            ("trustee".to_string(), "other".to_string()),
        ]);
        right.as_of = Some("2025-06-30".to_string());
        let frontier = build_frontier(&artifact, "h", &[left, right], None, None, None).unwrap();
        assert_eq!(frontier.history_requests.len(), 1);
        let request = &frontier.history_requests[0];
        assert_eq!(request.need, "prior_observations");
        assert_eq!(
            request.scope,
            BTreeMap::from([
                ("deal".to_string(), "D".to_string()),
                ("loan".to_string(), "L".to_string()),
                ("role".to_string(), "sponsor".to_string()),
            ])
        );
        assert_eq!(request.before.as_deref(), Some("2025-03-31"));
        assert_eq!(request.settles_with, ["same_slot"]);

        // A pair that already has same_slot needs no history.
        let artifact = solve(
            vec![review(
                "A",
                "B",
                &["same_slot", "name_equivalent"],
                "second_lineage",
            )],
            Vec::new(),
            Vec::new(),
        );
        assert!(
            build_frontier(&artifact, "h", &[], None, None, None)
                .unwrap()
                .history_requests
                .is_empty()
        );
    }

    #[test]
    fn no_field_is_named_how_command_or_recipe() {
        let artifact = solve(
            vec![review("A", "B", &["name_equivalent"], "second_lineage")],
            Vec::new(),
            vec![cluster("c", "standalone", &["A"])],
        );
        let frontier = build_frontier(&artifact, "h", &[], None, None, None).unwrap();
        let text = serde_json::to_string(&frontier).unwrap();
        for forbidden in ["\"how\"", "\"command\"", "\"recipe\"", "next_command"] {
            assert!(!text.contains(forbidden), "{forbidden} in {text}");
        }
    }
}
