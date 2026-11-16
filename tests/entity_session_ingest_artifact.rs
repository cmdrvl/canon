use assert_cmd::Command;
use canon::entity::publication::{EntityPublicationSnapshot, open_current_stream_generation};
use canon::session::{SESSION_EVIDENCE_STREAM_ID, SESSION_OBSERVATIONS_STREAM_ID};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn init_session(temp: &TempDir) -> std::path::PathBuf {
    let registry = temp.path().join("registry");
    let session = temp.path().join("session");
    canon_command()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "artifact intake",
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
    session
}

fn digest(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn artifact(artifact_id: &str, byte_hash: &str) -> Value {
    json!({
        "artifact_id": artifact_id,
        "as_of": "2026-11-16",
        "byte_hash": byte_hash,
        "derived_from": [],
        "kind": "warehouse_read",
        "media_type": "application/json",
        "observed_at": "2026-11-16T00:00:00Z",
        "payload_ref": "cas:blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "schema": "artifact.v0",
        "scope_ref": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "sensitivity": "test_restricted"
    })
}

fn receipt(artifact_id: &str) -> Value {
    json!({
        "artifacts": [artifact_id],
        "claim_state": "supported",
        "code": null,
        "coverage": "complete",
        "context": {
            "kind": "node",
            "node_id": "node:test",
            "run_id": "run:test"
        },
        "domain_disposition": "positive",
        "execution": "ok",
        "message": "complete",
        "observed_at": "2026-11-16T00:00:00Z",
        "schema": "receipt.v0"
    })
}

fn observation_rows(snapshot: &EntityPublicationSnapshot) -> Vec<Value> {
    let mut rows = Vec::new();
    for path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot.read_logical_file(path).expect("observation shard");
        rows.extend(
            std::str::from_utf8(bytes)
                .expect("observation UTF-8")
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| serde_json::from_str(line).expect("observation JSON")),
        );
    }
    rows
}

#[test]
fn ingest_accepts_verified_artifact_receipt_and_preserves_row_provenance() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let payload = json!({
        "catalog_key": "snowflake://DB.SCHEMA/SPONSORS",
        "rows": [{
            "obs_id": "row-1",
            "entity_type": "organisation",
            "surface": "Acme Holdings, LLC",
            "cik": "0000000123",
            "deal_cik": "1771234",
            "channel": "warehouse_read",
            "source_id": "query-1",
            "row_hash": "blake3:row-1"
        }]
    });
    let payload_bytes = serde_json::to_vec(&payload).expect("payload JSON");
    let payload_path = temp.path().join("payload.json");
    fs::write(&payload_path, &payload_bytes).expect("payload");
    let artifact_path = temp.path().join("artifact.json");
    fs::write(
        &artifact_path,
        serde_json::to_vec_pretty(&artifact(
            "artifact:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &digest(&payload_bytes),
        ))
        .unwrap(),
    )
    .expect("artifact");
    let receipt_path = temp.path().join("receipt.json");
    fs::write(
        &receipt_path,
        serde_json::to_vec_pretty(&receipt(
            "artifact:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ))
        .unwrap(),
    )
    .expect("receipt");
    let map_path = temp.path().join("observations.yaml");
    fs::write(
        &map_path,
        "obs_id: obs_id\nentity_type: entity_type\nsurface: surface\nkeys:\n  cik: cik\nsource:\n  channel: channel\n  id: source_id\norigin_template: 'cmbs-deal:{deal_cik}'\nprimary_key: [obs_id]\n",
    )
    .expect("column map");

    let output = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            payload_path.to_str().unwrap(),
            "--rows-field",
            "rows",
            "--column-map",
            map_path.to_str().unwrap(),
            "--artifact",
            artifact_path.to_str().unwrap(),
            "--receipt",
            receipt_path.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout).expect("output JSON");
    assert_eq!(result["accepted"], 1);
    assert_eq!(result["rejected"].as_array().unwrap().len(), 0);

    let snapshot = open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
        .expect("observation generation");
    let row = &observation_rows(&snapshot)[0];
    assert_eq!(
        row["source"]["artifact_id"],
        "artifact:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(row["source"]["result_digest"], digest(&payload_bytes));
    assert_eq!(
        row["source"]["receipt_digest"],
        digest(&fs::read(&receipt_path).unwrap())
    );
    assert_eq!(row["source"]["sensitivity"], "test_restricted");
    assert_eq!(
        row["source"]["catalog_key"],
        "snowflake://DB.SCHEMA/SPONSORS"
    );
    assert_eq!(row["source"]["columns"]["surface"], "surface");
    assert_eq!(row["source"]["columns"]["keys"]["cik"], "cik");
    assert_eq!(row["source"]["row_hash"], "blake3:row-1");
    assert_eq!(row["source"]["lineage"], "cmbs-deal:1771234");
}

#[test]
fn ingest_rejects_tampered_payload_before_writing_any_observation() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let payload = br#"{"rows":[{"obs_id":"row-1","entity_type":"organisation","surface":"Acme","channel":"warehouse","source_id":"q"}]}"#;
    let payload_path = temp.path().join("payload.json");
    fs::write(&payload_path, payload).expect("payload");
    let artifact_path = temp.path().join("artifact.json");
    fs::write(
        &artifact_path,
        serde_json::to_vec(&artifact(
            "artifact:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &digest(payload),
        ))
        .unwrap(),
    )
    .expect("artifact");
    fs::write(
        temp.path().join("observations.yaml"),
        "obs_id: obs_id\nentity_type: entity_type\nsurface: surface\nsource:\n  channel: channel\n  id: source_id\n",
    )
    .expect("column map");
    let tampered = temp.path().join("tampered.json");
    fs::write(
        &tampered,
        br#"{"rows":[{"obs_id":"row-1","entity_type":"organisation","surface":"Wrong","channel":"warehouse","source_id":"q"}]}"#,
    )
    .expect("tampered payload");

    let output = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            tampered.to_str().unwrap(),
            "--rows-field",
            "rows",
            "--column-map",
            temp.path().join("observations.yaml").to_str().unwrap(),
            "--artifact",
            artifact_path.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let result: Value = serde_json::from_slice(&output.get_output().stdout).expect("refusal JSON");
    assert_eq!(result["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(result["refusal"]["detail"]["code"], "artifact_byte_hash");
    assert!(open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID).is_err());
}

#[test]
fn assert_rows_map_binds_the_verified_payload_digest_to_evidence() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let payload = json!({
        "rows": [{
            "left": "row-1",
            "right": "row-2",
            "rule": "same filing",
            "channel": "warehouse_read",
            "source_id": "query-assert",
            "asserted_by": "agent:warehouse"
        }]
    });
    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    let payload_path = temp.path().join("evidence.json");
    fs::write(&payload_path, &payload_bytes).unwrap();
    let map_path = temp.path().join("evidence.yaml");
    fs::write(
        &map_path,
        "type:\n  literal: name_equivalent\nleft: left\nright: right\nasserted_by: asserted_by\nbasis:\n  rule: rule\nsource:\n  channel: channel\n  id: source_id\n",
    )
    .unwrap();

    let output = canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().unwrap(),
            payload_path.to_str().unwrap(),
            "--rows-field",
            "rows",
            "--column-map",
            map_path.to_str().unwrap(),
            "--expect-digest",
            &digest(&payload_bytes),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(result["accepted"], 1);

    let snapshot = open_current_stream_generation(&session, SESSION_EVIDENCE_STREAM_ID)
        .expect("evidence generation");
    let path = snapshot
        .logical_paths()
        .find(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
        .expect("evidence shard");
    let record: Value = serde_json::from_slice(
        snapshot
            .read_logical_file(path)
            .unwrap()
            .split(|byte| *byte == b'\n')
            .find(|line| !line.is_empty())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        record["provenance"][0]["content_hash"],
        digest(&payload_bytes)
    );
}

#[test]
fn plain_jsonl_has_no_external_provenance_defaults() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let input = temp.path().join("plain.jsonl");
    fs::write(
        &input,
        "{\"obs_id\":\"plain-1\",\"entity_type\":\"organisation\",\"surface\":\"Plain\",\"source\":{\"channel\":\"file\",\"file\":\"plain.jsonl\"}}\n",
    )
    .unwrap();
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
        .success();
    let snapshot =
        open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID).unwrap();
    let row = &observation_rows(&snapshot)[0];
    assert!(row["source"].get("catalog_key").is_none());
    assert!(row["source"].get("sensitivity").is_none());
    assert!(row["source"].get("result_digest").is_none());
}
