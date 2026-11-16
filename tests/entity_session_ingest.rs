use assert_cmd::Command;
use canon::{
    entity::publication::{EntityPublicationSnapshot, open_current_stream_generation},
    session::SESSION_OBSERVATIONS_STREAM_ID,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn run_and_log(label: &str, output: &assert_cmd::assert::Assert) {
    eprintln!(
        "[{label}] exit={} stdout={} stderr={}",
        output.get_output().status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.get_output().stdout),
        String::from_utf8_lossy(&output.get_output().stderr),
    );
}

fn json_stdout(output: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&output.get_output().stdout).expect("JSON stdout envelope")
}

fn init_session(temp: &TempDir) -> std::path::PathBuf {
    let registry = temp.path().join("registry");
    let session = temp.path().join("session");
    let output = canon_command()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "ingest observations",
            "--registry",
            registry.to_str().unwrap(),
            "--work-dir",
            session.to_str().unwrap(),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "4",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    run_and_log("session init", &output);
    session
}

fn observation(obs_id: &str, surface: &str, channel: &str, file: &str) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": null},
        "attrs": {"role": "borrower"},
        "context": {"as_of_date": "2025-06-30"},
        "source": {"channel": channel, "file": file}
    }))
    .unwrap()
}

fn ingest_file(session: &Path, input: &Path) -> assert_cmd::assert::Assert {
    canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            input.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
}

fn observation_rows(snapshot: &EntityPublicationSnapshot) -> BTreeMap<String, Value> {
    let mut rows = BTreeMap::new();
    for path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot
            .read_logical_file(path)
            .expect("observation shard bytes");
        for line in std::str::from_utf8(bytes)
            .expect("observation shard UTF-8")
            .lines()
        {
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(line).expect("observation shard JSON");
            rows.insert(value["obs_id"].as_str().unwrap().to_string(), value);
        }
    }
    rows
}

#[test]
fn session_ingest_unions_files_and_stdin_and_exact_repeat_is_idempotent() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let source_a = temp.path().join("source-a.jsonl");
    let source_b = temp.path().join("source-b.jsonl");
    fs::write(
        &source_a,
        format!(
            "{}\n{}\n",
            observation("obs-a", "  `  ", "file", "source-a.jsonl"),
            observation("obs-b", "Borrower B", "file", "source-a.jsonl")
        ),
    )
    .expect("source A");
    let stdin_rows = format!(
        "{}\n",
        observation("obs-c", "Borrower C", "mcp", "source-b.jsonl")
    );
    fs::write(&source_b, &stdin_rows).expect("source B");

    let first = ingest_file(&session, &source_a).success();
    run_and_log("ingest source A", &first);
    let first_payload = json_stdout(&first);
    assert_eq!(first_payload["version"], "canon.entity.session.ingest.v1");
    assert_eq!(first_payload["accepted"], 2);
    assert_eq!(first_payload["already_present"], 0);
    assert_eq!(first_payload["rejected"].as_array().unwrap().len(), 0);
    let first_generation = first_payload["generation_id"].clone();

    let second = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            "-",
            "--emit",
            "json",
        ])
        .write_stdin(stdin_rows)
        .assert()
        .success();
    run_and_log("ingest stdin source B", &second);
    let second_payload = json_stdout(&second);
    assert_eq!(second_payload["accepted"], 1);
    assert_eq!(second_payload["already_present"], 0);
    assert_ne!(second_payload["generation_id"], first_generation);

    let snapshot = open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
        .expect("observation stream");
    let rows = observation_rows(&snapshot);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows["obs-a"]["surface"], "`");
    assert_eq!(rows["obs-a"]["source"]["lineage"], "file:source-a.jsonl");
    assert_eq!(rows["obs-c"]["source"]["lineage"], "mcp:source-b.jsonl");

    let repeat = ingest_file(&session, &source_a).success();
    run_and_log("repeat source A", &repeat);
    let repeat_payload = json_stdout(&repeat);
    assert_eq!(repeat_payload["accepted"], 0);
    assert_eq!(repeat_payload["already_present"], 2);
    assert_eq!(
        repeat_payload["generation_id"],
        second_payload["generation_id"]
    );
    let repeat_snapshot = open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
        .expect("unchanged observation stream");
    assert_eq!(repeat_snapshot.generation_id, snapshot.generation_id);
}

#[test]
fn session_ingest_rejects_three_planted_rows_and_writes_the_other_997() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let seed = temp.path().join("seed.jsonl");
    fs::write(
        &seed,
        format!(
            "{}\n",
            observation("existing", "Existing Borrower", "file", "seed.jsonl")
        ),
    )
    .expect("seed observations");
    let seed_output = ingest_file(&session, &seed).success();
    run_and_log("seed existing observation", &seed_output);
    assert_eq!(json_stdout(&seed_output)["accepted"], 1);

    let mut lines = Vec::with_capacity(1_000);
    for line_number in 1..=1_000 {
        let line = match line_number {
            2 => "{not-valid-json".to_string(),
            500 => observation("empty-surface", "  ", "file", "bad.jsonl"),
            999 => observation("existing", "Conflicting Borrower", "file", "bad.jsonl"),
            _ => observation(
                &format!("batch-{line_number:04}"),
                &format!("Borrower {line_number:04}"),
                "file",
                "bad.jsonl",
            ),
        };
        lines.push(line);
    }
    let input = temp.path().join("bad-batch.jsonl");
    fs::write(&input, format!("{}\n", lines.join("\n"))).expect("bad batch");

    let output = ingest_file(&session, &input).code(1);
    run_and_log("bad batch", &output);
    let payload = json_stdout(&output);
    eprintln!(
        "[bad batch rejections] {}",
        serde_json::to_string(&payload["rejected"]).unwrap()
    );
    assert_eq!(payload["accepted"], 997);
    assert_eq!(payload["already_present"], 0);
    let rejected = payload["rejected"].as_array().unwrap();
    assert_eq!(rejected.len(), 3);
    assert_eq!(rejected[0]["line"], 2);
    assert_eq!(rejected[0]["code"], "invalid_json");
    assert_eq!(rejected[1]["line"], 500);
    assert_eq!(rejected[1]["code"], "empty_surface");
    assert_eq!(rejected[2]["line"], 999);
    assert_eq!(rejected[2]["code"], "obs_id_conflict");
    for item in rejected {
        assert!(item["fix"].as_str().is_some_and(|fix| !fix.is_empty()));
    }

    let snapshot = open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
        .expect("observation stream after partial ingest");
    assert_eq!(observation_rows(&snapshot).len(), 998);
}
