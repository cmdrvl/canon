use assert_cmd::Command;
use canon::entity::publication::{EntityPublicationSnapshot, open_current_stream_generation};
use canon::session::{SESSION_EVIDENCE_STREAM_ID, SESSION_OBSERVATIONS_STREAM_ID};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn status_json(session: &Path) -> Value {
    let output = canon_command()
        .args([
            "entity",
            "session",
            "status",
            session.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    serde_json::from_slice(&output.get_output().stdout).expect("status JSON")
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

fn published_schema_path(file_name: &str) -> PathBuf {
    Path::new("/Users/zac/Source/cmdrvl/evidence-protocol/schemas").join(file_name)
}

fn validate_published_schema(
    instance: &Value,
    schema: &Value,
    root_schema: &Value,
    path: &str,
) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let definition = reference
            .strip_prefix("#/$defs/")
            .and_then(|name| root_schema.get("$defs").and_then(|defs| defs.get(name)))
            .ok_or_else(|| format!("{path}: unresolved schema reference {reference}"))?;
        return validate_published_schema(instance, definition, root_schema, path);
    }

    if let Some(all_of) = schema.get("allOf").and_then(Value::as_array) {
        for (index, branch) in all_of.iter().enumerate() {
            validate_published_schema(
                instance,
                branch,
                root_schema,
                &format!("{path}.allOf[{index}]"),
            )?;
        }
    }
    if let Some(if_schema) = schema.get("if")
        && validate_published_schema(instance, if_schema, root_schema, path).is_ok()
        && let Some(then_schema) = schema.get("then")
    {
        validate_published_schema(instance, then_schema, root_schema, &format!("{path}.then"))?;
    }
    if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
        let matches = one_of
            .iter()
            .filter(|branch| validate_published_schema(instance, branch, root_schema, path).is_ok())
            .count();
        if matches != 1 {
            return Err(format!("{path}: oneOf matched {matches} branches"));
        }
    }
    if let Some(type_schema) = schema.get("type") {
        let matches = type_schema
            .as_str()
            .map(|type_name| schema_type_matches(instance, type_name))
            .or_else(|| {
                type_schema.as_array().map(|types| {
                    types.iter().any(|type_name| {
                        type_name
                            .as_str()
                            .is_some_and(|type_name| schema_type_matches(instance, type_name))
                    })
                })
            })
            .unwrap_or(false);
        if !matches {
            return Err(format!(
                "{path}: value does not match schema type {type_schema}"
            ));
        }
    }
    if let Some(constant) = schema.get("const")
        && instance != constant
    {
        return Err(format!(
            "{path}: value does not equal schema const {constant}"
        ));
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && !values.iter().any(|value| value == instance)
    {
        return Err(format!("{path}: value is not in schema enum"));
    }
    if let Some(min_length) = schema.get("minLength").and_then(Value::as_u64)
        && instance
            .as_str()
            .is_none_or(|value| value.chars().count() < min_length as usize)
    {
        return Err(format!("{path}: string is shorter than schema minLength"));
    }
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str)
        && instance
            .as_str()
            .is_none_or(|value| !schema_pattern_matches(value, pattern))
    {
        return Err(format!(
            "{path}: string does not match schema pattern {pattern}"
        ));
    }

    if let Some(object) = instance.as_object() {
        if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .ok_or_else(|| format!("{path}: schema has no properties for an object"))?;
            if let Some(unknown) = object.keys().find(|key| !properties.contains_key(*key)) {
                return Err(format!("{path}: unknown property {unknown}"));
            }
        }
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for field in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(field) {
                    return Err(format!("{path}: missing required property {field}"));
                }
            }
        }
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (field, field_schema) in properties {
                if let Some(value) = object.get(field) {
                    validate_published_schema(
                        value,
                        field_schema,
                        root_schema,
                        &format!("{path}.{field}"),
                    )?;
                }
            }
        }
    }
    if let Some(items_schema) = schema.get("items")
        && let Some(items) = instance.as_array()
    {
        for (index, item) in items.iter().enumerate() {
            validate_published_schema(
                item,
                items_schema,
                root_schema,
                &format!("{path}[{index}]"),
            )?;
        }
    }
    Ok(())
}

fn schema_type_matches(value: &Value, type_name: &str) -> bool {
    match type_name {
        "array" => value.is_array(),
        "null" => value.is_null(),
        "object" => value.is_object(),
        "string" => value.is_string(),
        _ => false,
    }
}

/// Evaluate the small anchored pattern language used by the published v0
/// schemas without embedding any artifact/ref rules in this test.
fn schema_pattern_matches(value: &str, pattern: &str) -> bool {
    let pattern = pattern
        .strip_prefix('^')
        .unwrap_or(pattern)
        .strip_suffix('$')
        .unwrap_or(pattern);
    let bytes = pattern.as_bytes();
    let value = value.as_bytes();
    let mut pattern_index = 0;
    let mut value_index = 0;
    while pattern_index < bytes.len() {
        let (matcher, next_index) = if bytes[pattern_index] == b'[' {
            let Some(close) = bytes[pattern_index + 1..]
                .iter()
                .position(|byte| *byte == b']')
                .map(|offset| pattern_index + 1 + offset)
            else {
                return false;
            };
            (
                PatternMatcher::Class(&bytes[pattern_index + 1..close]),
                close + 1,
            )
        } else if bytes[pattern_index] == b'\\' {
            if pattern_index + 1 >= bytes.len() {
                return false;
            }
            (
                PatternMatcher::Literal(bytes[pattern_index + 1]),
                pattern_index + 2,
            )
        } else {
            (
                PatternMatcher::Literal(bytes[pattern_index]),
                pattern_index + 1,
            )
        };
        let (repeat, next_index) = if next_index < bytes.len() && bytes[next_index] == b'{' {
            let Some(close) = bytes[next_index + 1..]
                .iter()
                .position(|byte| *byte == b'}')
                .map(|offset| next_index + 1 + offset)
            else {
                return false;
            };
            let Some(repeat) = std::str::from_utf8(&bytes[next_index + 1..close])
                .ok()
                .and_then(|text| text.parse::<usize>().ok())
            else {
                return false;
            };
            (repeat, close + 1)
        } else {
            (1, next_index)
        };
        for _ in 0..repeat {
            let Some(byte) = value.get(value_index).copied() else {
                return false;
            };
            if !matcher.matches(byte) {
                return false;
            }
            value_index += 1;
        }
        pattern_index = next_index;
    }
    value_index == value.len()
}

enum PatternMatcher<'a> {
    Class(&'a [u8]),
    Literal(u8),
}

impl PatternMatcher<'_> {
    fn matches(&self, value: u8) -> bool {
        match self {
            Self::Literal(expected) => value == *expected,
            Self::Class(class) => {
                let mut index = 0;
                while index < class.len() {
                    if index + 2 < class.len() && class[index + 1] == b'-' {
                        if class[index] <= value && value <= class[index + 2] {
                            return true;
                        }
                        index += 3;
                    } else {
                        if class[index] == value {
                            return true;
                        }
                        index += 1;
                    }
                }
                false
            }
        }
    }
}

#[test]
fn published_artifact_and_receipt_fixtures_validate_or_skip_by_name() {
    const TEST_NAME: &str = "published_artifact_and_receipt_fixtures_validate_or_skip_by_name";
    let schema_root = published_schema_path(".");
    if !schema_root.is_dir() {
        eprintln!("SKIPPED {TEST_NAME}: evidence-protocol schema checkout is absent");
        return;
    }
    for (file_name, fixture) in [
        (
            "artifact.v0.schema.json",
            artifact(
                "artifact:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
        ),
        (
            "receipt.v0.schema.json",
            receipt(
                "artifact:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        ),
    ] {
        let path = schema_root.join(file_name);
        if !path.is_file() {
            eprintln!("SKIPPED {TEST_NAME}: missing {}", path.display());
            return;
        }
        let schema: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        validate_published_schema(&fixture, &schema, &schema, "$")
            .unwrap_or_else(|error| panic!("{TEST_NAME}: {file_name}: {error}"));
    }
    eprintln!("VALIDATED {TEST_NAME}: artifact.v0 and receipt.v0 published schemas");
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
    assert_eq!(status_json(&session)["log"]["unreceipted"], 0);
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
        "rows": [
            {
                "left": "row-1",
                "right": "row-2",
                "rule": "same filing",
                "channel": "warehouse_read",
                "source_id": "query-assert-1",
                "asserted_by": "agent:warehouse"
            },
            {
                "left": "row-3",
                "right": "row-4",
                "rule": "same filing",
                "channel": "warehouse_read",
                "source_id": "query-assert-2",
                "asserted_by": "agent:warehouse"
            }
        ]
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
    assert_eq!(result["accepted"], 2);

    let snapshot = open_current_stream_generation(&session, SESSION_EVIDENCE_STREAM_ID)
        .expect("evidence generation");
    let path = snapshot
        .logical_paths()
        .find(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
        .expect("evidence shard");
    let records = snapshot
        .read_logical_file(path)
        .unwrap()
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        records.len(),
        2,
        "one evidence record must be written per row"
    );
    assert!(
        records
            .iter()
            .all(|record| { record["provenance"][0]["content_hash"] == digest(&payload_bytes) })
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
    assert!(
        status_json(&session)["log"]["unreceipted"]
            .as_u64()
            .unwrap()
            > 0
    );
}
