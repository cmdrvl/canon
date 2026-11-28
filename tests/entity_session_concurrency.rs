use assert_cmd::Command;
use canon::{
    entity::publication::{EntityPublicationSnapshot, open_current_stream_generation},
    session::{SESSION_EVIDENCE_STREAM_ID, SESSION_OBSERVATIONS_STREAM_ID},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command as ProcessCommand, Stdio},
};
use tempfile::TempDir;

const RETRY_DISABLED_TEST_FAILPOINT: &str = "CANON_TEST_FAILPOINT_SESSION_RETRY_DISABLED";
const PUBLISH_BARRIER_TEST_FAILPOINT: &str = "CANON_TEST_FAILPOINT_PUBLICATION_BARRIER";
const PUBLISH_BARRIER_PARTICIPANTS_TEST_FAILPOINT: &str =
    "CANON_TEST_FAILPOINT_PUBLICATION_PARTICIPANTS";
const PUBLISH_BARRIER_MARKER_TEST_FAILPOINT: &str = "CANON_TEST_FAILPOINT_PUBLICATION_MARKER_ID";

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn init_session(root: &Path, name: &str) -> PathBuf {
    let registry = root.join(format!("{name}-registry"));
    let session = root.join(name);
    let output = canon_command()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "concurrent session intake",
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
        .output()
        .expect("run session init");
    assert!(
        output.status.success(),
        "session init failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    session
}

fn observation(worker: usize, row: usize) -> String {
    serde_json::to_string(&json!({
        "obs_id": format!("obs-{worker:02}-{row:04}"),
        "entity_type": "organisation",
        "surface": format!("Organisation {worker:02}-{row:04}"),
        "keys": {"cik": null},
        "attrs": {"role": "borrower"},
        "context": {"as_of_date": "2025-06-30"},
        "source": {"channel": "file", "file": format!("observations-{worker:02}.jsonl")}
    }))
    .expect("observation JSON")
}

fn assertion(worker: usize, row: usize) -> String {
    let left = format!("left-{worker:02}-{row:04}");
    let right = format!("right-{worker:02}-{row:04}");
    serde_json::to_string(&json!({
        "type": "same_slot",
        "left": left,
        "right": right,
        "basis": {
            "rule": "same loan and role",
            "left_value": format!("loan-{worker:02}-{row:04}:sponsor"),
            "right_value": format!("loan-{worker:02}-{row:04}:sponsor"),
            "steps": ["join:loan_id", "join:role"]
        },
        "source": {"channel": "agent", "id": format!("parallel-{worker:02}")},
        "asserted_by": format!("agent:parallel-{worker:02}")
    }))
    .expect("assertion JSON")
}

fn conflicting_observation(surface: &str, file: &str) -> String {
    serde_json::to_string(&json!({
        "obs_id": "shared-observation",
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": null},
        "attrs": {"role": "borrower"},
        "context": {"as_of_date": "2025-06-30"},
        "source": {"channel": "file", "file": file}
    }))
    .expect("conflicting observation JSON")
}

#[derive(Clone, Copy)]
enum IntakeKind {
    Ingest,
    Assert,
}

struct Worker {
    kind: IntakeKind,
    id: usize,
    input: PathBuf,
}

fn write_input(root: &Path, kind: IntakeKind, worker: usize) -> PathBuf {
    let (stem, lines): (&str, Vec<String>) = match kind {
        IntakeKind::Ingest => (
            "observations",
            (0..500).map(|row| observation(worker, row)).collect(),
        ),
        IntakeKind::Assert => (
            "assertions",
            (0..500).map(|row| assertion(worker, row)).collect(),
        ),
    };
    let path = root.join(format!("{stem}-{worker:02}.jsonl"));
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write input JSONL");
    path
}

fn spawn_worker(
    session: &Path,
    gate: &Path,
    worker: &Worker,
    retry_disabled: bool,
    // barrier directory and this writer's place in it, unique within the batch
    publication_barrier: Option<(&Path, usize)>,
) -> Child {
    let subcommand = match worker.kind {
        IntakeKind::Ingest => "ingest",
        IntakeKind::Assert => "assert",
    };
    let mut command = ProcessCommand::new("/bin/sh");
    command.args([
        "-c",
        "while [ ! -f \"$1\" ]; do sleep 0.005; done; shift; exec \"$@\"",
        "canon-session-worker",
        gate.to_str().unwrap(),
        env!("CARGO_BIN_EXE_canon"),
        "entity",
        "session",
        subcommand,
        session.to_str().unwrap(),
        worker.input.to_str().unwrap(),
        "--emit",
        "json",
    ]);
    if retry_disabled {
        command.env(RETRY_DISABLED_TEST_FAILPOINT, "1");
    }
    if let Some((barrier, marker)) = publication_barrier {
        command
            .env(PUBLISH_BARRIER_TEST_FAILPOINT, barrier)
            .env(PUBLISH_BARRIER_PARTICIPANTS_TEST_FAILPOINT, "8")
            .env(PUBLISH_BARRIER_MARKER_TEST_FAILPOINT, marker.to_string());
    }
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn canon process")
}

fn run_concurrently(session: &Path, root: &Path, workers: Vec<Worker>) -> Vec<Value> {
    let session_name = session
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session");
    let gate = root.join(format!("start-workers-{session_name}"));
    // Every writer waits here, after reading the head and before claiming the
    // next generation, until all eight have arrived. Without it a fast or
    // lightly loaded machine can run the writers one after another, and the
    // retry this test is about never happens.
    let barrier = root.join(format!("publication-barrier-{session_name}"));
    assert_eq!(workers.len(), 8, "the barrier waits for eight writers");
    let mut children = Vec::with_capacity(workers.len());
    for (place, worker) in workers.into_iter().enumerate() {
        children.push((
            worker.id,
            worker.kind,
            spawn_worker(session, &gate, &worker, false, Some((&barrier, place))),
        ));
    }

    fs::write(&gate, b"start\n").expect("release process start gate");
    let outputs: Vec<Value> = children
        .into_iter()
        .map(|(id, kind, child)| {
            let output = child.wait_with_output().expect("wait for canon process");
            eprintln!(
                "[worker {id} process] exit={} stdout={} stderr={}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            let payload: Value =
                serde_json::from_slice(&output.stdout).expect("worker emitted valid JSON");
            eprintln!(
                "[worker {id} {:?}] exit={} attempts={} generation={} stdout={} stderr={}",
                match kind {
                    IntakeKind::Ingest => "ingest",
                    IntakeKind::Assert => "assert",
                },
                output.status.code().unwrap_or(-1),
                payload["attempts"],
                payload["generation_id"],
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            assert!(output.status.success(), "worker {id} failed");
            assert!(payload["attempts"].as_u64().unwrap_or(0) >= 1);
            assert!(payload["attempts"].as_u64().unwrap() <= 32);
            assert_eq!(payload["accepted"], 500, "worker {id} lost input rows");
            assert_eq!(payload["rejected"].as_array().unwrap().len(), 0);
            payload
        })
        .collect();
    assert!(
        outputs.iter().any(|payload| {
            payload["attempts"]
                .as_u64()
                .is_some_and(|attempts| attempts > 1)
        }),
        "the concurrent process batch must exercise a stale-head retry"
    );
    outputs
}

fn stream_hash(snapshot: &EntityPublicationSnapshot) -> String {
    let mut hasher = blake3::Hasher::new();
    for path in snapshot.logical_paths() {
        let bytes = snapshot.read_logical_file(path).expect("stream file bytes");
        hasher.update(&(path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    hasher.finalize().to_hex().to_string()
}

fn observation_count(snapshot: &EntityPublicationSnapshot) -> usize {
    snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
        .map(|path| {
            snapshot
                .read_logical_file(path)
                .expect("observation shard")
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .count()
        })
        .sum()
}

fn evidence_count(snapshot: &EntityPublicationSnapshot) -> usize {
    snapshot
        .logical_paths()
        .filter(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
        .map(|path| {
            snapshot
                .read_logical_file(path)
                .expect("evidence shard")
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .count()
        })
        .sum()
}

fn ingest_workers(root: &Path, ids: impl IntoIterator<Item = usize>) -> Vec<Worker> {
    ids.into_iter()
        .map(|id| Worker {
            kind: IntakeKind::Ingest,
            id,
            input: write_input(root, IntakeKind::Ingest, id),
        })
        .collect()
}

fn mixed_workers(root: &Path, reverse: bool) -> Vec<Worker> {
    let mut workers = Vec::with_capacity(8);
    for id in 0..4 {
        workers.push(Worker {
            kind: IntakeKind::Ingest,
            id,
            input: write_input(root, IntakeKind::Ingest, id),
        });
        workers.push(Worker {
            kind: IntakeKind::Assert,
            id,
            input: write_input(root, IntakeKind::Assert, id),
        });
    }
    if reverse {
        workers.reverse();
    }
    workers
}

fn conflicting_pair(root: &Path, session: &Path, reverse: bool) {
    let gate = root.join(format!(
        "start-conflict-{}",
        session
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("session")
    ));
    let inputs = [
        (0, "Conflict winner A", "conflict-a.jsonl"),
        (1, "Conflict winner B", "conflict-b.jsonl"),
    ]
    .map(|(id, surface, file)| {
        let input = root.join(format!("conflict-{id}.jsonl"));
        fs::write(
            &input,
            format!("{}\n", conflicting_observation(surface, file)),
        )
        .expect("write conflicting observation");
        Worker {
            kind: IntakeKind::Ingest,
            id,
            input,
        }
    });
    let order = if reverse { [1, 0] } else { [0, 1] };
    let mut children = Vec::with_capacity(2);
    for index in order {
        children.push((
            index,
            spawn_worker(session, &gate, &inputs[index], false, None),
        ));
    }
    fs::write(&gate, b"start\n").expect("release conflict processes");

    let mut results = Vec::with_capacity(2);
    for (id, child) in children {
        let output = child.wait_with_output().expect("wait for conflict process");
        eprintln!(
            "[conflict worker {id} process] exit={} stdout={} stderr={}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let payload: Value =
            serde_json::from_slice(&output.stdout).expect("conflict worker emitted valid JSON");
        eprintln!(
            "[conflict worker {id}] exit={} attempts={} generation={} stdout={} stderr={}",
            output.status.code().unwrap_or(-1),
            payload["attempts"],
            payload["generation_id"],
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(payload["attempts"].as_u64().unwrap_or(0) >= 1);
        assert!(payload["attempts"].as_u64().unwrap() <= 32);
        results.push((id, output.status.code(), payload));
    }

    let accepted = results
        .iter()
        .filter(|(_, _, payload)| payload["accepted"] == 1)
        .count();
    assert_eq!(accepted, 1, "exactly one conflicting observation wins");
    let loser = results
        .iter()
        .find(|(_, _, payload)| payload["accepted"] == 0)
        .expect("one conflicting writer loses");
    assert_eq!(loser.1, Some(1), "conflicting writer exits partial");
    assert_eq!(loser.2["rejected"][0]["code"], "obs_id_conflict");
    let winner = results
        .iter()
        .find(|(_, _, payload)| payload["accepted"] == 1)
        .unwrap();
    assert_eq!(winner.1, Some(0), "accepted writer exits successfully");

    let snapshot = open_current_stream_generation(session, SESSION_OBSERVATIONS_STREAM_ID)
        .expect("conflict observation stream");
    assert_eq!(observation_count(&snapshot), 1);
    let expected_surfaces = ["Conflict winner A", "Conflict winner B"];
    let expected_surface = *expected_surfaces
        .get(winner.0)
        .expect("winner id belongs to one of the two workers");
    let stored_line = snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
        .flat_map(|path| {
            snapshot
                .read_logical_file(path)
                .expect("observation shard")
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| String::from_utf8(line.to_vec()).expect("observation UTF-8"))
                .collect::<Vec<_>>()
        })
        .next()
        .expect("one stored observation");
    let stored_value: Value = serde_json::from_str(&stored_line).expect("stored observation JSON");
    assert_eq!(stored_value["surface"], expected_surface);
}

#[test]
fn concurrent_process_ingest_and_assert_preserve_all_rows_and_stable_stream_content() {
    let temp = TempDir::new().expect("temp root");
    let first_session = init_session(temp.path(), "ingest-forward");
    let first_workers = ingest_workers(temp.path(), 0..8);
    let first_outputs = run_concurrently(&first_session, temp.path(), first_workers);
    assert_eq!(first_outputs.len(), 8);
    let first_snapshot =
        open_current_stream_generation(&first_session, SESSION_OBSERVATIONS_STREAM_ID)
            .expect("first observation stream");
    assert_eq!(observation_count(&first_snapshot), 4_000);
    let first_hash = stream_hash(&first_snapshot);
    eprintln!(
        "[final observation stream] generation={} content_hash={first_hash}",
        first_snapshot.generation_id
    );

    let second_session = init_session(temp.path(), "ingest-reverse");
    let second_workers = ingest_workers(temp.path(), (0..8).rev());
    let second_outputs = run_concurrently(&second_session, temp.path(), second_workers);
    assert_eq!(second_outputs.len(), 8);
    let second_snapshot =
        open_current_stream_generation(&second_session, SESSION_OBSERVATIONS_STREAM_ID)
            .expect("second observation stream");
    assert_eq!(observation_count(&second_snapshot), 4_000);
    assert_eq!(stream_hash(&second_snapshot), first_hash);
    eprintln!(
        "[reversed final observation stream] generation={} content_hash={first_hash}",
        second_snapshot.generation_id
    );

    let mixed_a = init_session(temp.path(), "mixed-forward");
    let mixed_a_outputs =
        run_concurrently(&mixed_a, temp.path(), mixed_workers(temp.path(), false));
    assert_eq!(mixed_a_outputs.len(), 8);
    let mixed_a_observations =
        open_current_stream_generation(&mixed_a, SESSION_OBSERVATIONS_STREAM_ID)
            .expect("mixed observation stream");
    let mixed_a_evidence = open_current_stream_generation(&mixed_a, SESSION_EVIDENCE_STREAM_ID)
        .expect("mixed evidence stream");
    assert_eq!(observation_count(&mixed_a_observations), 2_000);
    assert_eq!(evidence_count(&mixed_a_evidence), 2_000);
    let mixed_observation_hash = stream_hash(&mixed_a_observations);
    let mixed_evidence_hash = stream_hash(&mixed_a_evidence);
    eprintln!(
        "[mixed final observation stream] generation={} content_hash={mixed_observation_hash}",
        mixed_a_observations.generation_id
    );
    eprintln!(
        "[mixed final evidence stream] generation={} content_hash={mixed_evidence_hash}",
        mixed_a_evidence.generation_id
    );

    let mixed_b = init_session(temp.path(), "mixed-reverse");
    let mixed_b_outputs = run_concurrently(&mixed_b, temp.path(), mixed_workers(temp.path(), true));
    assert_eq!(mixed_b_outputs.len(), 8);
    let mixed_b_observations =
        open_current_stream_generation(&mixed_b, SESSION_OBSERVATIONS_STREAM_ID)
            .expect("reversed mixed observation stream");
    let mixed_b_evidence = open_current_stream_generation(&mixed_b, SESSION_EVIDENCE_STREAM_ID)
        .expect("reversed mixed evidence stream");
    assert_eq!(observation_count(&mixed_b_observations), 2_000);
    assert_eq!(evidence_count(&mixed_b_evidence), 2_000);
    assert_eq!(stream_hash(&mixed_b_observations), mixed_observation_hash);
    assert_eq!(stream_hash(&mixed_b_evidence), mixed_evidence_hash);
    eprintln!(
        "[reversed mixed final observation stream] generation={} content_hash={mixed_observation_hash}",
        mixed_b_observations.generation_id
    );
    eprintln!(
        "[reversed mixed final evidence stream] generation={} content_hash={mixed_evidence_hash}",
        mixed_b_evidence.generation_id
    );
}

#[test]
fn concurrent_processes_without_retry_expose_lost_head_conflict() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(temp.path(), "retry-disabled");
    let seed_input = temp.path().join("retry-disabled-seed.jsonl");
    fs::write(&seed_input, format!("{}\n", observation(99, 0)))
        .expect("write retry-disabled seed observation");
    let seed_output = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            seed_input.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .output()
        .expect("seed retry-disabled session");
    assert!(
        seed_output.status.success(),
        "seed ingest failed: stdout={} stderr={}",
        String::from_utf8_lossy(&seed_output.stdout),
        String::from_utf8_lossy(&seed_output.stderr)
    );
    let seed_payload: Value = serde_json::from_slice(&seed_output.stdout).expect("seed JSON");
    let seed_generation = seed_payload["generation_id"]
        .as_str()
        .expect("seed generation id")
        .to_string();

    let workers = ingest_workers(temp.path(), 0..8);
    let gate = temp.path().join("start-retry-disabled-workers");
    let publication_barrier = temp.path().join("retry-disabled-publication-barrier");
    let mut children = Vec::with_capacity(workers.len());
    for worker in &workers {
        children.push((
            worker.id,
            spawn_worker(
                &session,
                &gate,
                worker,
                true,
                Some((&publication_barrier, worker.id)),
            ),
        ));
    }

    fs::write(&gate, b"start\n").expect("release retry-disabled process start gate");

    let mut successes = 0;
    let mut failures = 0;
    for (id, child) in children {
        let output = child.wait_with_output().expect("wait for canon process");
        let payload: Value =
            serde_json::from_slice(&output.stdout).expect("worker emitted valid JSON");
        eprintln!(
            "[retry-disabled worker {id}] exit={} stdout={} stderr={}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        if output.status.success() {
            successes += 1;
            assert_eq!(payload["attempts"], 1);
            assert_eq!(payload["accepted"], 500);
            assert_eq!(payload["rejected"].as_array().unwrap().len(), 0);
        } else {
            failures += 1;
            assert_eq!(output.status.code(), Some(2));
            let refusal = &payload["refusal"];
            assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
            assert_eq!(refusal["detail"]["attempts"], 1);
            assert_eq!(refusal["detail"]["kind"], "ForkedGeneration");
            assert_eq!(
                refusal["detail"]["expected_generation_id"].as_str(),
                Some(seed_generation.as_str())
            );
            assert_ne!(
                refusal["detail"]["observed_generation_id"].as_str(),
                Some(seed_generation.as_str())
            );
            assert!(
                refusal["message"]
                    .as_str()
                    .unwrap()
                    .contains("after 1 attempts")
            );
        }
    }

    assert!(successes > 0, "one writer must publish the initial head");
    assert!(
        failures > 0,
        "disabling retry must expose at least one stale-head writer failure"
    );
    let snapshot = open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
        .expect("retry-disabled writers published one observation stream");
    assert_eq!(observation_count(&snapshot), 1 + successes * 500);
}

#[test]
fn concurrent_conflicting_observation_id_has_one_winner_in_either_launch_order() {
    let temp = TempDir::new().expect("temp root");
    let forward = init_session(temp.path(), "conflict-forward");
    conflicting_pair(temp.path(), &forward, false);

    let reverse = init_session(temp.path(), "conflict-reverse");
    conflicting_pair(temp.path(), &reverse, true);
}
