use assert_cmd::Command;
use canon::entity::publication::{EntityPublicationSnapshot, open_current_stream_generation};
use canon::session::{SESSION_EVIDENCE_STREAM_ID, SESSION_OBSERVATIONS_STREAM_ID};
use parquet::{
    basic::Compression,
    data_type::{ByteArray, ByteArrayType},
    file::{
        properties::WriterProperties,
        reader::{FileReader, SerializedFileReader},
        writer::SerializedFileWriter,
    },
    schema::parser::parse_message_type,
};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
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

fn parquet_artifact(artifact_id: &str, byte_hash: &str) -> Value {
    let mut value = artifact(artifact_id, byte_hash);
    value["media_type"] = Value::String("application/vnd.apache.parquet".to_string());
    value
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

fn write_parquet(path: &Path, schema: &str, columns: &[&[&str]]) {
    let schema = Arc::new(parse_message_type(schema).expect("Parquet schema"));
    let properties = Arc::new(
        WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build(),
    );
    let file = fs::File::create(path).expect("create Parquet file");
    let mut writer = SerializedFileWriter::new(file, schema, properties).expect("Parquet writer");
    let mut row_group = writer.next_row_group().expect("Parquet row group");
    for values in columns {
        let mut column = row_group
            .next_column()
            .expect("Parquet column result")
            .expect("Parquet column");
        let values = values
            .iter()
            .map(|value| ByteArray::from(*value))
            .collect::<Vec<_>>();
        column
            .typed::<ByteArrayType>()
            .write_batch(&values, None, None)
            .expect("write Parquet values");
        column.close().expect("close Parquet column");
    }
    row_group.close().expect("close Parquet row group");
    writer.close().expect("close Parquet file");
}

fn init_place_export_session(temp: &TempDir) -> PathBuf {
    let mut kinds = serde_json::Map::new();
    for (kind, prefix) in [
        ("site", "SIT"),
        ("parcel", "PRC"),
        ("building", "BLD"),
        ("unit", "UNT"),
    ] {
        kinds.insert(
            kind.to_string(),
            json!({
                "registry": temp.path().join(kind),
                "registry_id": format!("{kind}s"),
                "prefix": prefix,
                "zero_pad": 3,
                "canonical_type": kind,
                "alias_file": "aliases.json"
            }),
        );
    }
    let registries_path = temp.path().join("registries.yaml");
    fs::write(
        &registries_path,
        serde_yaml::to_string(&json!({
            "version": "canon.session.registries.v1",
            "kinds": kinds
        }))
        .expect("registry map YAML"),
    )
    .expect("write registry map");

    let rules_path = temp.path().join("place-rules.yaml");
    fs::write(
        &rules_path,
        "version: canon.entity.session.rules.v1\nkeys:\n  apn: {authoritative: true, single_valued: true}\n  parcel_id: {authoritative: true, single_valued: true}\n  bin: {authoritative: true, single_valued: true}\n  unit_id: {authoritative: true, single_valued: true}\n  site_id: {authoritative: true, single_valued: true}\ntier_a:\n  evidence_types: [same_key, asserted_alias, same_apn, same_parcel_id]\ntier_b:\n  eligible_evidence_types: [address_canonical_match, point_in_parcel, footprint_overlap, deed_chain_link, assessor_corroboration]\n  min_distinct_evidence_types: 2\n  min_distinct_lineages: 2\nstandalone:\n  mint_when_authoritative_key: true\n",
    )
    .expect("write place rules");

    let session = temp.path().join("place-session");
    canon_command()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "Parquet place export round trip",
            "--work-dir",
        ])
        .arg(&session)
        .args(["--registries"])
        .arg(registries_path)
        .assert()
        .success();
    session
}

fn ingest_parquet_observations(session: &Path, path: &Path, schema: &str, columns: &[&[&str]]) {
    write_parquet(path, schema, columns);
    canon_command()
        .args(["entity", "session", "ingest"])
        .arg(session)
        .arg(path)
        .args(["--emit", "json"])
        .assert()
        .success();
}

fn export_registry(
    registry: &Path,
    format: &str,
    out: &Path,
    parquet: bool,
    namespace: Option<&str>,
) -> Value {
    let mut command = canon_command();
    command
        .args(["registry", "export", "--registry"])
        .arg(registry)
        .args(["--format", format, "--out"])
        .arg(out);
    if let Some(namespace) = namespace {
        command.args(["--namespace", namespace]);
    }
    if parquet {
        command.arg("--parquet");
    }
    let output = command.args(["--emit", "json"]).assert().success();
    serde_json::from_slice(&output.get_output().stdout).expect("registry export JSON")
}

fn parquet_rows(path: &Path) -> Vec<Value> {
    let reader = SerializedFileReader::new(fs::File::open(path).expect("open Parquet export"))
        .expect("read Parquet export");
    reader
        .get_row_iter(None)
        .expect("Parquet row iterator")
        .map(|row| row.expect("Parquet row").to_json_value())
        .collect()
}

fn lookup_export_roundtrip_id(root: &Path, registry: &Path, value: &str) -> String {
    let input = root.join("exact-lookup.csv");
    fs::write(&input, format!("value\n{value}\n")).expect("write lookup input");
    let output = canon_command()
        .arg(input)
        .args(["--registry"])
        .arg(registry)
        .args([
            "--column",
            "value",
            "--explicit",
            "--plain-json-values",
            "--no-witness",
        ])
        .output()
        .expect("exact lookup process");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).expect("lookup result JSON");
    result["mappings"][0]["canonical_id"]
        .as_str()
        .expect("resolved canonical id")
        .to_string()
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
        assert_eq!(
            validate_published_schema(&fixture, &schema, &schema, "$"),
            Ok(()),
            "{TEST_NAME}: {file_name}"
        );
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
fn parquet_session_ingest_and_assert_accept_canonical_rows_without_projection_options() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let observations_path = temp.path().join("canonical-observations.parquet");
    write_parquet(
        &observations_path,
        "message observations { REQUIRED BINARY obs_id (UTF8); REQUIRED BINARY entity_type (UTF8); REQUIRED BINARY surface (UTF8); REQUIRED group source { REQUIRED BINARY channel (UTF8); } }",
        &[
            &["obs-1"],
            &["organisation"],
            &["Acme Properties"],
            &["warehouse_read"],
        ],
    );
    let output = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            observations_path.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout).expect("ingest JSON");
    assert_eq!(result["accepted"], 1);
    let observation_snapshot =
        open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
            .expect("observation generation");
    let rows = observation_rows(&observation_snapshot);
    assert_eq!(rows[0]["obs_id"], "obs-1");
    assert_eq!(rows[0]["source"]["channel"], "warehouse_read");

    let evidence_path = temp.path().join("canonical-evidence.parquet");
    write_parquet(
        &evidence_path,
        "message evidence { REQUIRED BINARY type (UTF8); REQUIRED BINARY left (UTF8); REQUIRED BINARY right (UTF8); REQUIRED BINARY asserted_by (UTF8); REQUIRED group basis { REQUIRED BINARY rule (UTF8); } REQUIRED group source { REQUIRED BINARY channel (UTF8); REQUIRED BINARY id (UTF8); } }",
        &[
            &["name_equivalent"],
            &["obs-1"],
            &["obs-2"],
            &["agent:warehouse"],
            &["same filing"],
            &["warehouse_read"],
            &["query-1"],
        ],
    );
    let output = canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout).expect("assert JSON");
    assert_eq!(result["accepted"], 1);
    let evidence_snapshot = open_current_stream_generation(&session, SESSION_EVIDENCE_STREAM_ID)
        .expect("evidence generation");
    let evidence_shard = evidence_snapshot
        .logical_paths()
        .find(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
        .expect("evidence shard");
    let records = evidence_snapshot
        .read_logical_file(evidence_shard)
        .unwrap()
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["reason_code"], "name_equivalent");
}

#[test]
fn parquet_ingest_and_assert_verify_original_bytes_and_preserve_provenance() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let observations_path = temp.path().join("observations.parquet");
    write_parquet(
        &observations_path,
        "message observations { REQUIRED BINARY obs_id (UTF8); REQUIRED BINARY entity_type (UTF8); REQUIRED BINARY surface (UTF8); REQUIRED BINARY channel (UTF8); REQUIRED BINARY source_id (UTF8); }",
        &[
            &["obs-1"],
            &["organisation"],
            &["Acme Properties"],
            &["warehouse_read"],
            &["query-obs-1"],
        ],
    );
    let observation_bytes = fs::read(&observations_path).expect("observation Parquet bytes");
    let observation_artifact_id =
        "artifact:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let observation_artifact = temp.path().join("observation-artifact.json");
    fs::write(
        &observation_artifact,
        serde_json::to_vec_pretty(&parquet_artifact(
            observation_artifact_id,
            &digest(&observation_bytes),
        ))
        .unwrap(),
    )
    .expect("observation artifact");
    let observation_receipt = temp.path().join("observation-receipt.json");
    fs::write(
        &observation_receipt,
        serde_json::to_vec_pretty(&receipt(observation_artifact_id)).unwrap(),
    )
    .expect("observation receipt");
    let observation_map = temp.path().join("observations.yaml");
    fs::write(
        &observation_map,
        "obs_id: obs_id\nentity_type: entity_type\nsurface: surface\nsource:\n  channel: channel\n  id: source_id\n",
    )
    .expect("observation column map");

    let output = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            observations_path.to_str().unwrap(),
            "--rows-field",
            "rows",
            "--column-map",
            observation_map.to_str().unwrap(),
            "--expect-digest",
            &digest(&observation_bytes),
            "--artifact",
            observation_artifact.to_str().unwrap(),
            "--receipt",
            observation_receipt.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout).expect("ingest JSON");
    assert_eq!(result["accepted"], 1);
    let observation_snapshot =
        open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
            .expect("observation generation");
    let row = &observation_rows(&observation_snapshot)[0];
    assert_eq!(row["surface"], "Acme Properties");
    assert_eq!(row["source"]["result_digest"], digest(&observation_bytes));
    assert_eq!(row["source"]["artifact_id"], observation_artifact_id);
    assert_eq!(
        row["source"]["receipt_digest"],
        digest(&fs::read(&observation_receipt).unwrap())
    );

    let evidence_path = temp.path().join("evidence.parquet");
    write_parquet(
        &evidence_path,
        "message evidence { REQUIRED BINARY left (UTF8); REQUIRED BINARY right (UTF8); REQUIRED BINARY rule (UTF8); REQUIRED BINARY channel (UTF8); REQUIRED BINARY source_id (UTF8); REQUIRED BINARY asserted_by (UTF8); }",
        &[
            &["obs-1", "obs-2"],
            &["obs-2", "obs-3"],
            &["same filing", "same filing"],
            &["warehouse_read", "warehouse_read"],
            &["query-evidence-1", "query-evidence-2"],
            &["agent:warehouse", "agent:warehouse"],
        ],
    );
    let evidence_bytes = fs::read(&evidence_path).expect("evidence Parquet bytes");
    let evidence_artifact_id =
        "artifact:blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    let evidence_artifact = temp.path().join("evidence-artifact.json");
    fs::write(
        &evidence_artifact,
        serde_json::to_vec_pretty(&parquet_artifact(
            evidence_artifact_id,
            &digest(&evidence_bytes),
        ))
        .unwrap(),
    )
    .expect("evidence artifact");
    let evidence_receipt = temp.path().join("evidence-receipt.json");
    fs::write(
        &evidence_receipt,
        serde_json::to_vec_pretty(&receipt(evidence_artifact_id)).unwrap(),
    )
    .expect("evidence receipt");
    let evidence_map = temp.path().join("evidence.yaml");
    fs::write(
        &evidence_map,
        "type:\n  literal: name_equivalent\nleft: left\nright: right\nasserted_by: asserted_by\nbasis:\n  rule: rule\nsource:\n  channel: channel\n  id: source_id\n",
    )
    .expect("evidence column map");

    let output = canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
            "--rows-field",
            "rows",
            "--column-map",
            evidence_map.to_str().unwrap(),
            "--expect-digest",
            &digest(&evidence_bytes),
            "--artifact",
            evidence_artifact.to_str().unwrap(),
            "--receipt",
            evidence_receipt.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout).expect("assert JSON");
    assert_eq!(result["accepted"], 2);
    let evidence_snapshot = open_current_stream_generation(&session, SESSION_EVIDENCE_STREAM_ID)
        .expect("evidence generation");
    let evidence_path = evidence_snapshot
        .logical_paths()
        .find(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
        .expect("evidence shard");
    let records = evidence_snapshot
        .read_logical_file(evidence_path)
        .unwrap()
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|record| { record["provenance"][0]["content_hash"] == digest(&evidence_bytes) })
    );

    let rejected_path = temp.path().join("wrong-digest.parquet");
    write_parquet(
        &rejected_path,
        "message observations { REQUIRED BINARY obs_id (UTF8); REQUIRED BINARY entity_type (UTF8); REQUIRED BINARY surface (UTF8); REQUIRED BINARY channel (UTF8); REQUIRED BINARY source_id (UTF8); }",
        &[
            &["obs-rejected"],
            &["organisation"],
            &["Must Not Publish"],
            &["warehouse_read"],
            &["query-rejected"],
        ],
    );
    let rejected = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            rejected_path.to_str().unwrap(),
            "--rows-field",
            "rows",
            "--column-map",
            observation_map.to_str().unwrap(),
            "--expect-digest",
            &digest(&observation_bytes),
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let refusal: Value =
        serde_json::from_slice(&rejected.get_output().stdout).expect("refusal JSON");
    assert_eq!(refusal["refusal"]["detail"]["code"], "digest_mismatch");
    let unchanged = open_current_stream_generation(&session, SESSION_OBSERVATIONS_STREAM_ID)
        .expect("unchanged observations");
    assert_eq!(observation_rows(&unchanged).len(), 1);
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

#[test]
fn parquet_place_observations_round_trip_through_existing_registry_exports() {
    let temp = TempDir::new().expect("temp root");
    let session = init_place_export_session(&temp);
    let parcel_input = temp.path().join("parcel-observations.parquet");
    ingest_parquet_observations(
        &session,
        &parcel_input,
        "message observations { REQUIRED BINARY obs_id (UTF8); REQUIRED BINARY entity_type (UTF8); REQUIRED BINARY surface (UTF8); REQUIRED group keys { REQUIRED BINARY apn (UTF8); REQUIRED BINARY parcel_id (UTF8); } REQUIRED group source { REQUIRED BINARY channel (UTF8); REQUIRED BINARY id (UTF8); REQUIRED BINARY lineage (UTF8); } }",
        &[
            &["parcel-row-1", "parcel-row-2"],
            &["parcel", "parcel"],
            &["Parcel 1022-0043", "Tax lot 1022-0043"],
            &["1022-0043", "1022-0043"],
            &["BBL-1022-0043", "BBL-1022-0043"],
            &["warehouse_read", "warehouse_read"],
            &["county-parcel-1", "assessor-roll-1"],
            &["county-parcels:2026-11", "assessor-roll:2026-11"],
        ],
    );
    // a lone record with two authoritative keys stays standalone, so the parcel
    // needs evidence joining its two source records
    let same_parcel = temp.path().join("same-parcel.jsonl");
    fs::write(
        &same_parcel,
        format!(
            "{}\n",
            json!({
                "type": "same_apn",
                "left": "parcel-row-1",
                "right": "parcel-row-2",
                "asserted_by": "agent:warehouse",
                "basis": {"apn": "1022-0043"},
                "source": {
                    "channel": "warehouse_read",
                    "id": "assessor-roll-match",
                    "lineage": "assessor-roll:2026-11",
                    "locator": "assessor-roll:row-1",
                    "content_hash": format!("blake3:{}", "c".repeat(64))
                }
            })
        ),
    )
    .expect("write parcel assertion");
    canon_command()
        .args(["entity", "session", "assert"])
        .arg(&session)
        .arg(&same_parcel)
        .args(["--emit", "json"])
        .assert()
        .success();
    let building_input = temp.path().join("building-observations.parquet");
    ingest_parquet_observations(
        &session,
        &building_input,
        "message observations { REQUIRED BINARY obs_id (UTF8); REQUIRED BINARY entity_type (UTF8); REQUIRED BINARY surface (UTF8); REQUIRED group keys { REQUIRED BINARY bin (UTF8); } REQUIRED group source { REQUIRED BINARY channel (UTF8); REQUIRED BINARY id (UTF8); REQUIRED BINARY lineage (UTF8); } }",
        &[
            &["building-row-1"],
            &["building"],
            &["Paramount Plaza"],
            &["BIN-3112312"],
            &["warehouse_read"],
            &["county-building-1"],
            &["county-buildings:2026-11"],
        ],
    );
    let unit_input = temp.path().join("unit-observations.parquet");
    ingest_parquet_observations(
        &session,
        &unit_input,
        "message observations { REQUIRED BINARY obs_id (UTF8); REQUIRED BINARY entity_type (UTF8); REQUIRED BINARY surface (UTF8); REQUIRED group keys { REQUIRED BINARY unit_id (UTF8); } REQUIRED group source { REQUIRED BINARY channel (UTF8); REQUIRED BINARY id (UTF8); REQUIRED BINARY lineage (UTF8); } }",
        &[
            &["unit-row-1"],
            &["unit"],
            &["Suite 3600"],
            &["UNIT-1633B-3600"],
            &["warehouse_read"],
            &["county-unit-1"],
            &["county-units:2026-11"],
        ],
    );
    let site_input = temp.path().join("site-observations.parquet");
    ingest_parquet_observations(
        &session,
        &site_input,
        "message observations { REQUIRED BINARY obs_id (UTF8); REQUIRED BINARY entity_type (UTF8); REQUIRED BINARY surface (UTF8); REQUIRED group keys { REQUIRED BINARY site_id (UTF8); } REQUIRED group source { REQUIRED BINARY channel (UTF8); REQUIRED BINARY id (UTF8); REQUIRED BINARY lineage (UTF8); } }",
        &[
            &["site-row-1"],
            &["site"],
            &["Paramount Plaza Site"],
            &["SITE-PP"],
            &["warehouse_read"],
            &["county-site-1"],
            &["county-sites:2026-11"],
        ],
    );

    let relation_input = temp.path().join("relations.parquet");
    write_parquet(
        &relation_input,
        "message evidence { REQUIRED BINARY type (UTF8); REQUIRED BINARY left (UTF8); REQUIRED BINARY right (UTF8); REQUIRED BINARY asserted_by (UTF8); REQUIRED group basis { REQUIRED BINARY relation (UTF8); REQUIRED BINARY subject (UTF8); } REQUIRED group source { REQUIRED BINARY channel (UTF8); REQUIRED BINARY id (UTF8); REQUIRED BINARY lineage (UTF8); REQUIRED BINARY locator (UTF8); REQUIRED BINARY content_hash (UTF8); } }",
        &[
            &["related", "related"],
            &["unit-row-1", "building-row-1"],
            &["building-row-1", "site-row-1"],
            &["agent:warehouse", "agent:warehouse"],
            &["part_of", "part_of"],
            // the child of each `part_of` edge
            &["unit-row-1", "building-row-1"],
            &["warehouse_read", "warehouse_read"],
            &["unit-building-hierarchy", "building-site-hierarchy"],
            &["place-hierarchy:2026-11", "place-hierarchy:2026-11"],
            &[
                "place-hierarchy:unit-building",
                "place-hierarchy:building-site",
            ],
            &[
                "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ],
        ],
    );
    let asserted = canon_command()
        .args(["entity", "session", "assert"])
        .arg(&session)
        .arg(&relation_input)
        .args(["--emit", "json"])
        .assert()
        .success();
    let asserted: Value =
        serde_json::from_slice(&asserted.get_output().stdout).expect("assert JSON");
    assert_eq!(asserted["accepted"], 2);

    let rules = temp.path().join("place-rules.yaml");
    let solve = canon_command()
        .args(["entity", "session", "solve"])
        .arg(&session)
        .args(["--rules"])
        .arg(&rules)
        .args(["--emit", "json"])
        .assert()
        .success();
    let solve: Value = serde_json::from_slice(&solve.get_output().stdout).expect("solve JSON");
    assert_eq!(solve["summary"]["contradiction"], 0, "{solve}");
    let committed = canon_command()
        .args(["entity", "session", "commit"])
        .arg(&session)
        .args(["--bump", "patch", "--emit", "json"])
        .assert()
        .success();
    let committed: Value =
        serde_json::from_slice(&committed.get_output().stdout).expect("commit JSON");
    assert_eq!(committed["outcome"], "committed", "{committed}");

    let parcel_registry = temp.path().join("parcel");
    let building_registry = temp.path().join("building");
    let unit_registry = temp.path().join("unit");
    let site_registry = temp.path().join("site");
    let parcel_id = lookup_export_roundtrip_id(temp.path(), &parcel_registry, "1022-0043");
    let building_id = lookup_export_roundtrip_id(temp.path(), &building_registry, "BIN-3112312");
    let unit_id = lookup_export_roundtrip_id(temp.path(), &unit_registry, "UNIT-1633B-3600");
    let site_id = lookup_export_roundtrip_id(temp.path(), &site_registry, "SITE-PP");

    let seed_one = temp.path().join("places-one.parquet");
    let seed_two = temp.path().join("places-two.parquet");
    let seed_one_result = export_registry(
        &parcel_registry,
        "dbt-seed",
        &seed_one,
        true,
        Some("warehouse_places"),
    );
    let seed_two_result = export_registry(
        &parcel_registry,
        "dbt-seed",
        &seed_two,
        true,
        Some("warehouse_places"),
    );
    assert_eq!(seed_one_result["format"], "dbt-seed");
    assert_eq!(
        seed_one_result["content_hash"],
        seed_two_result["content_hash"]
    );
    assert!(
        seed_one_result["content_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("blake3:"))
    );
    assert_eq!(fs::read(&seed_one).unwrap(), fs::read(&seed_two).unwrap());
    let seed_rows = parquet_rows(&seed_one);
    assert!(seed_rows.iter().any(|row| {
        row["canonical_id"] == parcel_id
            && row["canonical_type"] == "parcel"
            && row["rule_id"] == "SESSION_KEY_APN"
    }));
    assert!(seed_rows.iter().any(|row| {
        row["canonical_id"] == parcel_id
            && row["canonical_type"] == "parcel"
            && row["rule_id"] == "SESSION_KEY_PARCEL_ID"
    }));
    for (kind, registry, canonical_id) in [
        ("building", &building_registry, &building_id),
        ("unit", &unit_registry, &unit_id),
        ("site", &site_registry, &site_id),
    ] {
        let out = temp.path().join(format!("{kind}-seed.parquet"));
        export_registry(registry, "dbt-seed", &out, true, Some("warehouse_places"));
        assert!(
            parquet_rows(&out).iter().any(|row| {
                row["canonical_id"] == *canonical_id && row["canonical_type"] == kind
            })
        );
    }

    let seed_csv = temp.path().join("places.csv");
    let seed_csv_result = export_registry(
        &parcel_registry,
        "dbt-seed",
        &seed_csv,
        false,
        Some("warehouse_places"),
    );
    assert_eq!(seed_csv_result["format"], "dbt-seed");
    assert_ne!(
        seed_csv_result["content_hash"],
        seed_one_result["content_hash"]
    );
    let csv_bytes = fs::read(&seed_csv).expect("default CSV seed remains available");
    assert!(csv_bytes.starts_with(b"namespace,source_input,normalized_key,"));

    // the dbt schema and anti-collapse test do not depend on the seed's encoding
    let scaffold = |parquet: bool| -> (String, String) {
        let directory = temp.path().join(format!("scaffold-{parquet}"));
        let schema = directory.join("schema.yml");
        let anti_collapse = directory.join("anti_collapse.sql");
        let mut command = canon_command();
        command
            .args(["registry", "export", "--registry"])
            .arg(&parcel_registry)
            .args(["--format", "dbt-seed", "--namespace", "warehouse_places"])
            .arg("--out")
            .arg(directory.join("places"))
            .arg("--schema-out")
            .arg(&schema)
            .arg("--anti-collapse-test-out")
            .arg(&anti_collapse);
        if parquet {
            command.arg("--parquet");
        }
        command.assert().success();
        (
            fs::read_to_string(schema).expect("dbt schema"),
            fs::read_to_string(anti_collapse).expect("anti-collapse test"),
        )
    };
    assert_eq!(scaffold(true), scaffold(false));
    let mut ids_by_key = std::collections::BTreeMap::<String, String>::new();
    for row in &seed_rows {
        let key = format!("{}|{}", row["namespace"], row["normalized_key"]);
        let id = row["canonical_id"].to_string();
        assert_eq!(
            ids_by_key.entry(key).or_insert_with(|| id.clone()),
            &id,
            "{row}"
        );
    }

    let crosswalk_one = temp.path().join("crosswalk-one.parquet");
    let crosswalk_two = temp.path().join("crosswalk-two.parquet");
    let crosswalk_one_result = export_registry(
        &parcel_registry,
        "key-crosswalk",
        &crosswalk_one,
        true,
        None,
    );
    let crosswalk_two_result = export_registry(
        &parcel_registry,
        "key-crosswalk",
        &crosswalk_two,
        true,
        None,
    );
    assert_eq!(
        crosswalk_one_result["content_hash"],
        crosswalk_two_result["content_hash"]
    );
    assert_eq!(
        fs::read(&crosswalk_one).unwrap(),
        fs::read(&crosswalk_two).unwrap()
    );
    assert_eq!(crosswalk_one_result["format"], "key-crosswalk");
    assert!(
        crosswalk_one_result["content_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("blake3:"))
    );
    let crosswalk_rows = parquet_rows(&crosswalk_one);
    let parcel_crosswalk = crosswalk_rows
        .iter()
        .find(|row| row["canonical_id"] == parcel_id)
        .expect("parcel appears in the key crosswalk");
    assert_eq!(parcel_crosswalk["canonical_type"], "parcel");
    assert_eq!(parcel_crosswalk["apn"], "1022-0043");
    assert_eq!(parcel_crosswalk["parcel_id"], "BBL-1022-0043");
    for (kind, registry, canonical_id, key_name, key_value) in [
        (
            "building",
            &building_registry,
            &building_id,
            "bin",
            "BIN-3112312",
        ),
        (
            "unit",
            &unit_registry,
            &unit_id,
            "unit_id",
            "UNIT-1633B-3600",
        ),
        ("site", &site_registry, &site_id, "site_id", "SITE-PP"),
    ] {
        let out = temp.path().join(format!("{kind}-crosswalk.parquet"));
        export_registry(registry, "key-crosswalk", &out, true, None);
        let rows = parquet_rows(&out);
        let row = rows
            .iter()
            .find(|row| row["canonical_id"] == *canonical_id)
            .expect("place grain appears in the key crosswalk");
        assert_eq!(row["canonical_type"], kind);
        assert_eq!(row[key_name], key_value);
    }

    let crosswalk_csv = temp.path().join("crosswalk.csv");
    let crosswalk_csv_result = export_registry(
        &parcel_registry,
        "key-crosswalk",
        &crosswalk_csv,
        false,
        None,
    );
    assert_eq!(crosswalk_csv_result["format"], "key-crosswalk");
    assert_ne!(
        crosswalk_csv_result["content_hash"],
        crosswalk_one_result["content_hash"]
    );
    let csv_bytes = fs::read(&crosswalk_csv).expect("default CSV key crosswalk remains available");
    assert!(csv_bytes.starts_with(b"canonical_id,canonical_type,primary_name,apn,parcel_id\n"));
    assert!(
        String::from_utf8(csv_bytes)
            .expect("key crosswalk CSV is UTF-8")
            .lines()
            .any(|line| line.ends_with(",1022-0043,BBL-1022-0043"))
    );

    for (kind, registry, canonical_id, keys, parent_id) in [
        (
            "parcel",
            &parcel_registry,
            &parcel_id,
            vec![("apn", "1022-0043"), ("parcel_id", "BBL-1022-0043")],
            None,
        ),
        (
            "building",
            &building_registry,
            &building_id,
            vec![("bin", "BIN-3112312")],
            None,
        ),
        (
            "unit",
            &unit_registry,
            &unit_id,
            vec![("unit_id", "UNIT-1633B-3600")],
            Some(building_id.as_str()),
        ),
        (
            "site",
            &site_registry,
            &site_id,
            vec![("site_id", "SITE-PP")],
            None,
        ),
    ] {
        let index_path = temp.path().join(format!("{kind}-search.sqlite"));
        export_registry(registry, "search-index", &index_path, false, None);
        let index = Connection::open(&index_path).expect("open search index");
        let indexed_type: String = index
            .query_row(
                "SELECT canonical_type FROM entities WHERE canonical_id = ?1",
                [canonical_id],
                |row| row.get(0),
            )
            .expect("place grain is projected into the search index");
        assert_eq!(indexed_type, kind);
        for (namespace, expected_value) in keys {
            let indexed_key: String = index
                .query_row(
                    "SELECT key_value FROM external_keys WHERE canonical_id = ?1 AND key_namespace = ?2",
                    rusqlite::params![canonical_id, namespace],
                    |row| row.get(0),
                )
                .expect("place key reaches the search index");
            assert_eq!(indexed_key, expected_value);
        }
        // Each relation appears in the export of the registry that stores it:
        // unit -> building with the unit, building -> site with the site.
        let related = |context_kind: &str| -> Vec<String> {
            index
                .prepare(
                    "SELECT counterpart_id FROM entity_context WHERE canonical_id = ?1 AND kind = ?2",
                )
                .expect("entity context query")
                .query_map(rusqlite::params![canonical_id, context_kind], |row| {
                    row.get(0)
                })
                .expect("entity context rows")
                .map(|row| row.expect("counterpart id"))
                .collect()
        };
        assert_eq!(
            related("part_of"),
            parent_id.into_iter().collect::<Vec<_>>(),
            "{kind}"
        );
        let children = if kind == "site" {
            vec![building_id.as_str()]
        } else {
            Vec::new()
        };
        assert_eq!(related("unit"), children, "{kind}");
    }

    let unsupported_path = temp.path().join("unsupported-search.parquet");
    let unsupported = canon_command()
        .args(["registry", "export", "--registry"])
        .arg(&unit_registry)
        .args(["--format", "search-index", "--out"])
        .arg(&unsupported_path)
        .args(["--parquet", "--emit", "json"])
        .output()
        .expect("unsupported-format refusal process");
    assert_eq!(unsupported.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&unsupported.stdout).expect("refusal JSON");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert!(!unsupported_path.exists());
}
