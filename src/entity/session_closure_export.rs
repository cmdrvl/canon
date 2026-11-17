#![forbid(unsafe_code)]

//! `canon entity session export --format evidence-closure`: the evidence behind
//! each outcome of a commit, written as deterministic local files that
//! `pack seal` can seal. Canon seals, uploads and indexes nothing here; the
//! closure refers to upstream inputs by handle and copies no source bytes.
//!
//! The manifest is the one the commit stored (`commit/closures/<hash>.json`),
//! written byte for byte: nothing is derived a second time.

use crate::{
    Refusal, RefusalCode,
    entity::{
        publication::{EntityPublicationSnapshot, open_current_stream_generation},
        session::SESSION_SOLVE_STREAM_ID,
        session_explain::read_solve_hashed,
    },
    session::{
        SESSION_COMMIT_STREAM_ID, SESSION_EVIDENCE_STREAM_ID, SESSION_OBSERVATIONS_STREAM_ID,
    },
    witness::hash_bytes,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const SESSION_EVIDENCE_CLOSURE_EXPORT_VERSION: &str =
    "canon.entity.session.evidence_closure_export.v0";

#[derive(Debug, Clone)]
pub struct SessionClosureExportRequest {
    pub session: PathBuf,
    pub out: PathBuf,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionClosureExportOutput {
    pub version: String,
    pub out: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_json: Option<ExportedFile>,
    pub closures: Vec<ExportedClosure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedFile {
    pub path: String,
    pub blake3: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedClosure {
    pub outcome_ref: String,
    pub closure_hash: String,
    pub directory: String,
    pub sensitivity: ClosureSensitivity,
    pub files: Vec<ExportedFile>,
}

/// `level` is `public` only when every input states `public` and none is
/// unstated. An input with no stated sensitivity is never treated as public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureSensitivity {
    pub level: String,
    pub values: Vec<String>,
    pub unstated_inputs: usize,
}

impl SessionClosureExportOutput {
    pub fn render_summary(&self) -> String {
        let mut lines = vec![format!(
            "evidence-closure out={} closures={}",
            self.out,
            self.closures.len()
        )];
        for closure in &self.closures {
            lines.push(format!(
                "closure outcome_ref={} closure_hash={} sensitivity={} unstated_inputs={} files={}",
                closure.outcome_ref,
                closure.closure_hash,
                closure.sensitivity.level,
                closure.sensitivity.unstated_inputs,
                closure.files.len()
            ));
        }
        lines.join("\n")
    }
}

pub fn sensitivity_of(stated: &[Option<String>]) -> ClosureSensitivity {
    let unstated_inputs = stated.iter().filter(|value| value.is_none()).count();
    let values = stated
        .iter()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let all_public =
        !stated.is_empty() && unstated_inputs == 0 && values.iter().all(|value| value == "public");
    ClosureSensitivity {
        level: if all_public { "public" } else { "restricted" }.to_string(),
        values,
        unstated_inputs,
    }
}

fn refusal(message: impl Into<String>, detail: Value, next: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.into(),
        detail,
        next_command: Some(next.to_string()),
    }
}

fn open_stream(
    session: &Path,
    stream: &str,
    required: bool,
) -> Result<Option<EntityPublicationSnapshot>, Refusal> {
    match open_current_stream_generation(session, stream) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) => {
            if required {
                Err(refusal(
                    format!("No committed {stream} generation is available: {error}"),
                    json!({"stream": stream, "writes_performed": false}),
                    "Run canon entity session commit, then rerun the export",
                ))
            } else {
                Ok(None)
            }
        }
    }
}

fn shard_lines(snapshot: &EntityPublicationSnapshot, prefix: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut paths = snapshot
        .logical_paths()
        .filter(|path| path.starts_with(prefix) && path.ends_with(".jsonl"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        if let Some(bytes) = snapshot.read_logical_file(&path)
            && let Ok(text) = std::str::from_utf8(bytes)
        {
            lines.extend(
                text.lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(str::to_string),
            );
        }
    }
    lines
}

fn handle(source: &Value, key: &str) -> Value {
    source.get(key).cloned().unwrap_or(Value::Null)
}

/// The references an observation carries to its upstream input. A handle the
/// observation does not carry is `null`; nothing is invented.
fn source_handles(record: &Value) -> Value {
    let source = record.get("source").cloned().unwrap_or(Value::Null);
    json!({
        "artifact_id": handle(&source, "artifact_id"),
        "receipt_digest": handle(&source, "receipt_digest"),
        "result_digest": handle(&source, "result_digest"),
        "catalog_key": handle(&source, "catalog_key"),
        "columns": handle(&source, "columns"),
        "row_hash": handle(&source, "row_hash"),
    })
}

fn stated_sensitivity(record: &Value) -> Option<String> {
    record
        .get("source")
        .and_then(|source| source.get("sensitivity"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn jsonl_bytes(values: &[Value]) -> Result<Vec<u8>, Refusal> {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend(serde_json::to_vec(value).map_err(|error| {
            refusal(
                format!("Unable to serialize a closure record: {error}"),
                json!({"writes_performed": false}),
                "Rerun canon entity session export",
            )
        })?);
        bytes.push(b'\n');
    }
    Ok(bytes)
}

pub fn export_evidence_closure(
    request: SessionClosureExportRequest,
) -> Result<SessionClosureExportOutput, Refusal> {
    let session = if request.session.is_absolute() {
        request.session.clone()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&request.session))
            .map_err(|error| {
                refusal(
                    format!("Unable to resolve the session path: {error}"),
                    json!({"writes_performed": false}),
                    "Pass an absolute session path",
                )
            })?
    };
    let commit = match open_current_stream_generation(&session, SESSION_COMMIT_STREAM_ID) {
        Ok(snapshot) => snapshot,
        Err(_) => {
            return Err(refusal(
                "The session has no commit; there is no outcome to export evidence for",
                json!({"reason": "no_commit", "writes_performed": false}),
                "canon entity session commit <SESSION> --next-version <VER>",
            ));
        }
    };
    let set_bytes = commit.read_logical_file("commit/closure-set.json");
    let set_value = set_bytes
        .map(serde_json::from_slice::<Value>)
        .transpose()
        .map_err(|error| {
            refusal(
                format!("The stored closure set is invalid: {error}"),
                json!({"writes_performed": false}),
                "Repair the session commit stream",
            )
        })?;
    let Some(set_value) = set_value else {
        return Err(refusal(
            "The latest commit wrote no outcome closures",
            json!({"reason": "no_closures", "writes_performed": false}),
            "canon entity session commit <SESSION> --next-version <VER>",
        ));
    };
    let mut closure_refs = set_value
        .get("closures")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            Some((
                entry.get("outcome_ref")?.as_str()?.to_string(),
                entry.get("closure_hash")?.as_str()?.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    closure_refs.sort();
    if let Some(wanted) = &request.outcome {
        closure_refs.retain(|(outcome_ref, _)| outcome_ref == wanted);
        if closure_refs.is_empty() {
            return Err(refusal(
                format!("The latest commit has no outcome '{wanted}'"),
                json!({"reason": "unknown_outcome", "outcome": wanted, "writes_performed": false}),
                "canon entity session export <SESSION> --format evidence-closure --out <DIR>",
            ));
        }
    }

    // Records the closures point into.
    let observations = open_stream(&session, SESSION_OBSERVATIONS_STREAM_ID, true)?
        .map(|snapshot| shard_lines(&snapshot, "observations/"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .collect::<Vec<_>>();
    let mut evidence = BTreeMap::<String, Value>::new();
    if let Some(snapshot) = open_stream(&session, SESSION_EVIDENCE_STREAM_ID, false)? {
        for line in shard_lines(&snapshot, "evidence/") {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && let Some(id) = value.get("evidence_id").and_then(Value::as_str)
            {
                evidence.entry(id.to_string()).or_insert(value);
            }
        }
    }
    let (solve, _) = read_solve_hashed(&session)?;
    for record in &solve.derived_evidence {
        if let Ok(value) = serde_json::to_value(record) {
            evidence.entry(record.evidence_id.clone()).or_insert(value);
        }
    }
    let rules_text = open_stream(&session, SESSION_SOLVE_STREAM_ID, false)?.and_then(|snapshot| {
        snapshot
            .read_logical_file("solve/rules.yaml")
            .map(<[u8]>::to_vec)
    });

    let mut exported = Vec::new();
    let mut staged: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for (outcome_ref, closure_hash) in &closure_refs {
        let hex = closure_hash
            .strip_prefix("blake3:")
            .unwrap_or(closure_hash.as_str())
            .to_string();
        let stored = commit
            .read_logical_file(&format!("commit/closures/{closure_hash}.json"))
            .ok_or_else(|| {
                refusal(
                    format!("The stored closure {closure_hash} is missing"),
                    json!({"closure_hash": closure_hash, "writes_performed": false}),
                    "Repair the session commit stream",
                )
            })?;
        let manifest: Value = serde_json::from_slice(stored).map_err(|error| {
            refusal(
                format!("The stored closure {closure_hash} is invalid: {error}"),
                json!({"closure_hash": closure_hash, "writes_performed": false}),
                "Repair the session commit stream",
            )
        })?;

        // Observation records named by the manifest: by obs_id, or by surface
        // when the closure kept only the surfaces.
        let mut obs_ids = BTreeSet::new();
        let mut surfaces = BTreeSet::new();
        for entry in manifest
            .get("observations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(id) = entry.get("obs_id").and_then(Value::as_str) {
                obs_ids.insert(id.to_string());
            } else if let Some(surface) = entry.get("surface").and_then(Value::as_str) {
                surfaces.insert(surface.to_string());
            }
        }
        let mut involved = observations
            .iter()
            .filter(|record| {
                record
                    .get("obs_id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| obs_ids.contains(id))
                    || (obs_ids.is_empty()
                        && record
                            .get("surface")
                            .and_then(Value::as_str)
                            .is_some_and(|surface| surfaces.contains(surface)))
            })
            .cloned()
            .collect::<Vec<_>>();
        involved.sort_by(|left, right| {
            left.get("obs_id")
                .and_then(Value::as_str)
                .cmp(&right.get("obs_id").and_then(Value::as_str))
        });
        involved.dedup_by(|left, right| left.get("obs_id") == right.get("obs_id"));
        for id in &obs_ids {
            if !involved
                .iter()
                .any(|record| record.get("obs_id").and_then(Value::as_str) == Some(id))
            {
                return Err(refusal(
                    format!(
                        "Observation {id} named by closure {closure_hash} is not in the session"
                    ),
                    json!({"obs_id": id, "closure_hash": closure_hash, "writes_performed": false}),
                    "Repair the session observation stream",
                ));
            }
        }
        let observation_lines = involved
            .iter()
            .map(|record| {
                json!({
                    "obs_id": record.get("obs_id").cloned().unwrap_or(Value::Null),
                    "record": record,
                    "source_handles": source_handles(record),
                })
            })
            .collect::<Vec<_>>();

        let mut evidence_lines = Vec::new();
        for id in manifest
            .get("evidence_ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let record = evidence.get(id).ok_or_else(|| {
                refusal(
                    format!("Evidence {id} named by closure {closure_hash} is not in the session"),
                    json!({"evidence_id": id, "closure_hash": closure_hash, "writes_performed": false}),
                    "Repair the session evidence stream",
                )
            })?;
            evidence_lines.push(record.clone());
        }

        let sensitivity =
            sensitivity_of(&involved.iter().map(stated_sensitivity).collect::<Vec<_>>());

        let directory = PathBuf::from(&hex);
        let mut files = vec![
            ("closure.json".to_string(), stored.to_vec()),
            (
                "observations.jsonl".to_string(),
                jsonl_bytes(&observation_lines)?,
            ),
            ("evidence.jsonl".to_string(), jsonl_bytes(&evidence_lines)?),
        ];
        if let Some(rules) = &rules_text {
            files.push(("rules.yaml".to_string(), rules.clone()));
        }
        let mut listed = Vec::new();
        for (name, bytes) in files {
            listed.push(ExportedFile {
                path: format!("{hex}/{name}"),
                blake3: hash_bytes(&bytes),
                bytes: bytes.len(),
            });
            staged.push((directory.join(&name), bytes));
        }
        exported.push(ExportedClosure {
            outcome_ref: outcome_ref.clone(),
            closure_hash: closure_hash.clone(),
            directory: hex,
            sensitivity,
            files: listed,
        });
    }

    let commit_json = if request.outcome.is_none() {
        set_bytes.map(|bytes| {
            staged.push((PathBuf::from("commit.json"), bytes.to_vec()));
            ExportedFile {
                path: "commit.json".to_string(),
                blake3: hash_bytes(bytes),
                bytes: bytes.len(),
            }
        })
    } else {
        None
    };

    for (relative, bytes) in &staged {
        let target = request.out.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                refusal(
                    format!("Unable to create {}: {error}", parent.display()),
                    json!({"writes_performed": true}),
                    "Choose a writable --out directory",
                )
            })?;
        }
        fs::write(&target, bytes).map_err(|error| {
            refusal(
                format!("Unable to write {}: {error}", target.display()),
                json!({"writes_performed": true}),
                "Choose a writable --out directory",
            )
        })?;
    }
    Ok(SessionClosureExportOutput {
        version: SESSION_EVIDENCE_CLOSURE_EXPORT_VERSION.to_string(),
        out: request.out.display().to_string(),
        commit_json,
        closures: exported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    #[test]
    fn public_only_when_every_input_states_public() {
        let all_public = sensitivity_of(&[some("public"), some("public")]);
        assert_eq!(all_public.level, "public");
        assert_eq!(all_public.values, ["public"]);
        assert_eq!(all_public.unstated_inputs, 0);
    }

    #[test]
    fn an_unstated_input_is_never_public() {
        let unstated = sensitivity_of(&[None]);
        assert_eq!(unstated.level, "restricted");
        assert_eq!(unstated.unstated_inputs, 1);
        let mixed_with_unstated = sensitivity_of(&[some("public"), None]);
        assert_eq!(mixed_with_unstated.level, "restricted");
        assert_eq!(mixed_with_unstated.unstated_inputs, 1);
        assert_eq!(mixed_with_unstated.values, ["public"]);
    }

    #[test]
    fn one_restricted_input_makes_the_closure_restricted() {
        let mixed = sensitivity_of(&[some("test_restricted"), some("public"), some("public")]);
        assert_eq!(mixed.level, "restricted");
        assert_eq!(mixed.values, ["public", "test_restricted"]);
        assert_eq!(mixed.unstated_inputs, 0);
    }

    #[test]
    fn no_inputs_is_not_public() {
        assert_eq!(sensitivity_of(&[]).level, "restricted");
    }

    #[test]
    fn handles_a_record_does_not_carry_are_null() {
        let handles = source_handles(&json!({"source": {"channel": "file", "lineage": "a"}}));
        for key in [
            "artifact_id",
            "receipt_digest",
            "catalog_key",
            "columns",
            "row_hash",
        ] {
            assert!(handles[key].is_null(), "{key}");
        }
        let kept = source_handles(&json!({"source": {"artifact_id": "a1", "columns": ["x"]}}));
        assert_eq!(kept["artifact_id"], "a1");
        assert_eq!(kept["columns"], json!(["x"]));
        assert!(kept["row_hash"].is_null());
    }
}
