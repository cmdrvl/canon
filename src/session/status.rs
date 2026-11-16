//! Entity-free core of `session status` (bd-vdcw.5).
//!
//! Everything here works from stream ids, generation ids, hashes and the
//! commit receipt. It knows no entity type, so the Geo session status
//! (bd-vdcw.42) can reuse it. Nothing in this module writes: a status is
//! computed on demand and never stored.

use crate::{
    Refusal, RefusalCode,
    entity::publication::{
        EntityPublicationErrorKind, EntityPublicationSnapshot, open_current_stream_generation,
    },
    session::{SESSION_COMMIT_VERSION, SESSION_MANIFEST_STREAM_ID, SessionManifest},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeSet, path::Path};

const COMMIT_RECEIPT_PATH: &str = "commit/commit.json";
const MANIFEST_PATH: &str = "session/session.json";

/// Why a recorded solve no longer describes the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleReason {
    NewObservations,
    NewEvidence,
    RulesChanged,
    RegistryChanged,
}

/// The inputs a solve recorded about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedInputs {
    pub observations_generation: String,
    pub evidence_generation: Option<String>,
    pub rules_hash: String,
    /// Opaque registry identity: id, version and snapshot hash joined.
    pub registry: String,
}

/// The same inputs read from the session as it is now. `rules_hash` is only
/// known when the caller names the rules file; without it rule drift cannot be
/// seen and is not reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentInputs {
    pub observations_generation: Option<String>,
    pub evidence_generation: Option<String>,
    pub rules_hash: Option<String>,
    pub registry: String,
}

/// Compare a solve's recorded inputs with the current stream heads.
///
/// `registry_change_is_own_commit` excuses a registry difference that the
/// session's own commit of this very solve explains.
pub fn stale_reasons(
    recorded: &RecordedInputs,
    current: &CurrentInputs,
    registry_change_is_own_commit: bool,
) -> Vec<StaleReason> {
    let mut reasons = Vec::new();
    if current.observations_generation.as_deref() != Some(recorded.observations_generation.as_str())
    {
        reasons.push(StaleReason::NewObservations);
    }
    if current.evidence_generation != recorded.evidence_generation {
        reasons.push(StaleReason::NewEvidence);
    }
    if current
        .rules_hash
        .as_deref()
        .is_some_and(|hash| hash != recorded.rules_hash)
    {
        reasons.push(StaleReason::RulesChanged);
    }
    if current.registry != recorded.registry && !registry_change_is_own_commit {
        reasons.push(StaleReason::RegistryChanged);
    }
    reasons
}

/// What the agent should run next, as one command string.
#[derive(Debug, Clone)]
pub struct NextCommandInputs<'a> {
    pub session: &'a str,
    pub observations_present: bool,
    pub solve_present: bool,
    pub stale: bool,
    pub uncommitted_accepted_clusters: usize,
    pub rules: Option<&'a str>,
}

/// `None` when nothing is pending: only commands that exist are ever emitted.
pub fn next_command(inputs: &NextCommandInputs<'_>) -> Option<String> {
    let session = inputs.session;
    if !inputs.observations_present {
        return Some(format!(
            "canon entity session ingest {session} <OBSERVATIONS.jsonl>"
        ));
    }
    if !inputs.solve_present || inputs.stale {
        return Some(format!(
            "canon entity session solve {session} --rules {}",
            inputs.rules.unwrap_or("<RULES.yaml>")
        ));
    }
    if inputs.uncommitted_accepted_clusters > 0 {
        return Some(format!(
            "canon entity session commit {session} --bump patch"
        ));
    }
    None
}

/// The session manifest, or an `E_PARSE` refusal when `work_dir` is not a
/// session. Reads only.
pub fn read_manifest(work_dir: &Path) -> Result<SessionManifest, Refusal> {
    let snapshot = match open_current_stream_generation(work_dir, SESSION_MANIFEST_STREAM_ID) {
        Ok(snapshot) => snapshot,
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => {
            return Err(not_a_session(work_dir));
        }
        Err(error) => {
            return Err(Refusal {
                code: RefusalCode::EEntityArtifactContract,
                message: format!("Unable to read the session manifest: {}", error.message),
                detail: json!({
                    "session": work_dir.display().to_string(),
                    "kind": format!("{:?}", error.kind),
                    "writes_performed": false,
                }),
                next_command: Some(
                    "Repair the session publication or point at another session".to_string(),
                ),
            });
        }
    };
    let bytes = snapshot
        .read_logical_file(MANIFEST_PATH)
        .ok_or_else(|| not_a_session(work_dir))?;
    serde_json::from_slice(bytes).map_err(|error| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Session manifest is not valid JSON: {error}"),
        detail: json!({"session": work_dir.display().to_string(), "writes_performed": false}),
        next_command: Some(
            "Repair the session publication or point at another session".to_string(),
        ),
    })
}

fn not_a_session(work_dir: &Path) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: format!(
            "'{}' is not an evidence session (no session manifest was published there)",
            work_dir.display()
        ),
        detail: json!({"session": work_dir.display().to_string(), "writes_performed": false}),
        next_command: Some(format!(
            "Run canon entity session init --work-dir {} ...",
            work_dir.display()
        )),
    }
}

/// The current head of a stream, `None` when nothing was ever published to it.
pub fn open_stream_head(
    work_dir: &Path,
    stream_id: &str,
) -> Result<Option<EntityPublicationSnapshot>, Refusal> {
    match open_current_stream_generation(work_dir, stream_id) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => Ok(None),
        Err(error) => Err(Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: format!(
                "Unable to read session stream {stream_id}: {}",
                error.message
            ),
            detail: json!({
                "stream": stream_id,
                "kind": format!("{:?}", error.kind),
                "writes_performed": false,
            }),
            next_command: Some("Repair the session publication, then rerun status".to_string()),
        }),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ReceiptRegistry {
    version_after: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReceiptMinted {
    cluster_id: String,
    #[serde(default)]
    aliases: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReceiptAttached {
    #[serde(default)]
    aliases: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReceiptSkipped {
    cluster_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Receipt {
    version: String,
    solve_hash: String,
    registry: ReceiptRegistry,
    #[serde(default)]
    minted: Vec<ReceiptMinted>,
    #[serde(default)]
    attached: Vec<ReceiptAttached>,
    #[serde(default)]
    skipped: Vec<ReceiptSkipped>,
}

/// What the last commit receipt says, without entity types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitHead {
    pub generation_id: String,
    pub solve_hash: String,
    pub registry_version_after: String,
    pub minted: usize,
    pub aliases_attached: usize,
    pub minted_cluster_ids: BTreeSet<String>,
    pub skipped_cluster_ids: BTreeSet<String>,
}

pub fn read_commit_head(snapshot: &EntityPublicationSnapshot) -> Result<CommitHead, Refusal> {
    let bytes = snapshot
        .read_logical_file(COMMIT_RECEIPT_PATH)
        .ok_or_else(|| commit_receipt_refusal("the generation has no commit/commit.json"))?;
    let receipt: Receipt = serde_json::from_slice(bytes)
        .map_err(|error| commit_receipt_refusal(&format!("it is not valid JSON: {error}")))?;
    if receipt.version != SESSION_COMMIT_VERSION {
        return Err(commit_receipt_refusal(&format!(
            "unexpected receipt version {}",
            receipt.version
        )));
    }
    Ok(CommitHead {
        generation_id: snapshot.generation_id.clone(),
        solve_hash: receipt.solve_hash,
        registry_version_after: receipt.registry.version_after,
        minted: receipt.minted.len(),
        aliases_attached: receipt
            .minted
            .iter()
            .map(|entry| entry.aliases.len())
            .chain(receipt.attached.iter().map(|entry| entry.aliases.len()))
            .sum(),
        minted_cluster_ids: receipt
            .minted
            .iter()
            .map(|entry| entry.cluster_id.clone())
            .collect(),
        skipped_cluster_ids: receipt
            .skipped
            .iter()
            .map(|entry| entry.cluster_id.clone())
            .collect(),
    })
}

fn commit_receipt_refusal(why: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Session commit receipt is unusable: {why}"),
        detail: json!({"stream": "entity-session-commit", "writes_performed": false}),
        next_command: Some("Repair the session commit stream, then rerun status".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded() -> RecordedInputs {
        RecordedInputs {
            observations_generation: "blake3:obs1".to_string(),
            evidence_generation: Some("blake3:ev1".to_string()),
            rules_hash: "blake3:rules1".to_string(),
            registry: "orgs@1.0.0:snap1".to_string(),
        }
    }

    fn matching() -> CurrentInputs {
        CurrentInputs {
            observations_generation: Some("blake3:obs1".to_string()),
            evidence_generation: Some("blake3:ev1".to_string()),
            rules_hash: Some("blake3:rules1".to_string()),
            registry: "orgs@1.0.0:snap1".to_string(),
        }
    }

    #[test]
    fn identical_inputs_are_fresh() {
        assert!(stale_reasons(&recorded(), &matching(), false).is_empty());
    }

    #[test]
    fn each_changed_input_names_its_own_reason() {
        let mut current = matching();
        current.observations_generation = Some("blake3:obs2".to_string());
        assert_eq!(
            stale_reasons(&recorded(), &current, false),
            vec![StaleReason::NewObservations]
        );

        let mut current = matching();
        current.evidence_generation = Some("blake3:ev2".to_string());
        assert_eq!(
            stale_reasons(&recorded(), &current, false),
            vec![StaleReason::NewEvidence]
        );

        let mut current = matching();
        current.rules_hash = Some("blake3:rules2".to_string());
        assert_eq!(
            stale_reasons(&recorded(), &current, false),
            vec![StaleReason::RulesChanged]
        );

        let mut current = matching();
        current.registry = "orgs@1.0.1:snap2".to_string();
        assert_eq!(
            stale_reasons(&recorded(), &current, false),
            vec![StaleReason::RegistryChanged]
        );
    }

    #[test]
    fn first_evidence_after_a_solve_without_evidence_is_new_evidence() {
        let mut recorded = recorded();
        recorded.evidence_generation = None;
        assert_eq!(
            stale_reasons(&recorded, &matching(), false),
            vec![StaleReason::NewEvidence]
        );
    }

    #[test]
    fn unknown_rules_are_not_reported_as_changed() {
        let mut current = matching();
        current.rules_hash = None;
        assert!(stale_reasons(&recorded(), &current, false).is_empty());
    }

    #[test]
    fn own_commit_excuses_only_the_registry_difference() {
        let mut current = matching();
        current.registry = "orgs@1.0.1:snap2".to_string();
        assert!(stale_reasons(&recorded(), &current, true).is_empty());

        current.observations_generation = Some("blake3:obs2".to_string());
        assert_eq!(
            stale_reasons(&recorded(), &current, true),
            vec![StaleReason::NewObservations]
        );
    }

    #[test]
    fn several_reasons_are_reported_together() {
        let mut current = matching();
        current.observations_generation = Some("blake3:obs2".to_string());
        current.evidence_generation = None;
        assert_eq!(
            stale_reasons(&recorded(), &current, false),
            vec![StaleReason::NewObservations, StaleReason::NewEvidence]
        );
    }

    fn inputs() -> NextCommandInputs<'static> {
        NextCommandInputs {
            session: "work/s",
            observations_present: true,
            solve_present: true,
            stale: false,
            uncommitted_accepted_clusters: 0,
            rules: None,
        }
    }

    #[test]
    fn next_command_follows_the_loop() {
        let mut state = inputs();
        state.observations_present = false;
        state.solve_present = false;
        assert!(
            next_command(&state)
                .unwrap()
                .starts_with("canon entity session ingest work/s")
        );

        let mut state = inputs();
        state.solve_present = false;
        assert!(
            next_command(&state)
                .unwrap()
                .starts_with("canon entity session solve work/s --rules")
        );

        let mut state = inputs();
        state.stale = true;
        state.uncommitted_accepted_clusters = 3;
        state.rules = Some("rules.yaml");
        assert_eq!(
            next_command(&state).as_deref(),
            Some("canon entity session solve work/s --rules rules.yaml"),
            "a stale solve outranks uncommitted clusters"
        );

        let mut state = inputs();
        state.uncommitted_accepted_clusters = 2;
        assert_eq!(
            next_command(&state).as_deref(),
            Some("canon entity session commit work/s --bump patch")
        );

        assert_eq!(
            next_command(&inputs()),
            None,
            "nothing pending, no phantom command"
        );
    }
}
