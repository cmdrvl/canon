use assert_cmd::Command;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
use tempfile::TempDir;

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

#[test]
fn entity_card_without_relation_or_memory_sidecars_has_empty_context() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("bare");
    write_json(
        &registry.join("registry.json"),
        &json!({
            "id": "bare-fixture",
            "version": "1.0.0",
            "description": "bare entity card fixture",
            "updated": "2026-11-16",
            "entry_count": 1
        }),
    );
    write_json(
        &registry.join("aliases.json"),
        &json!([{
            "input": "Solo Holdings",
            "canonical_id": "ORG-001",
            "canonical_type": "firm",
            "rule_id": "SESSION_NAME"
        }]),
    );
    let out = temp.path().join("cards");
    canon()
        .args([
            "registry",
            "export",
            "--registry",
            registry.to_str().unwrap(),
            "--format",
            "entity-card",
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    let card = card(&out, "ORG-001");
    assert_eq!(card["part_of"], json!([]));
    assert_eq!(card["units"], json!([]));
    assert_eq!(card["distinct"], json!([]));
    assert!(card["evidence"].is_null());
}

fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn seed_registry(dir: &Path, many_aliases: bool) {
    let mut entries = vec![
        json!({
            "input": "Alpha Holdings",
            "canonical_id": "ORG-001",
            "canonical_type": "firm",
            "rule_id": "SESSION_NAME",
            "session_authority": {
                "outcome_ref": "cmdrvl://session/alpha",
                "closure_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }
        }),
        json!({
            "input": "Alpha CIK",
            "canonical_id": "ORG-001",
            "canonical_type": "firm",
            "rule_id": "SESSION_NAME_VARIANT"
        }),
        json!({
            "input": "111",
            "canonical_id": "ORG-001",
            "canonical_type": "firm",
            "rule_id": "SESSION_KEY_CIK"
        }),
        json!({
            "input": "legacy-alpha-key",
            "canonical_id": "ORG-001",
            "canonical_type": "firm",
            "rule_id": "LEGACY_KEY"
        }),
        json!({
            "input": "Beta Holdings",
            "canonical_id": "ORG-002",
            "canonical_type": "firm",
            "rule_id": "SESSION_NAME"
        }),
        json!({
            "input": "222",
            "canonical_id": "ORG-002",
            "canonical_type": "firm",
            "rule_id": "SESSION_KEY_CIK"
        }),
        json!({
            "input": "Operations Unit",
            "canonical_id": "UNIT-001",
            "canonical_type": "unit",
            "rule_id": "SESSION_NAME"
        }),
        json!({
            "input": "org_agent_alpha",
            "canonical_id": "ORG-001",
            "canonical_type": "firm",
            "rule_id": "SESSION_AGENT_ID"
        }),
        json!({
            "input": "ORG-009",
            "canonical_id": "ORG-001",
            "canonical_type": "firm",
            "rule_id": "SUPERSEDED_ID_ALIAS"
        }),
    ];
    if many_aliases {
        for index in 0..40 {
            entries.push(json!({
                "input": format!("Alpha alias {index:02}"),
                "canonical_id": "ORG-001",
                "canonical_type": "firm",
                "rule_id": "SESSION_NAME_VARIANT"
            }));
        }
    }
    write_json(
        &dir.join("registry.json"),
        &json!({
            "id": "card-fixture",
            "version": "1.0.0",
            "description": "entity card fixture",
            "updated": "2026-11-16",
            "entry_count": entries.len()
        }),
    );
    write_json(&dir.join("aliases.json"), &Value::Array(entries));
    let relation = json!({
        "version": "canon.entity.relation.v1",
        "subject": {"identity_id": "UNIT-001"},
        "relation": {"value": "part_of"},
        "object": {"identity_id": "ORG-001"}
    });
    fs::write(
        dir.join("relations.jsonl"),
        format!("{}\n", serde_json::to_string(&relation).unwrap()),
    )
    .unwrap();
    let mut distinct = Vec::new();
    for index in 0..12 {
        distinct.push(json!({
            "left_key": "registry:alias:Alpha Holdings",
            "right_key": if index == 0 {
                "registry:alias:Beta Holdings".to_string()
            } else {
                format!("registry:alias:Beta look-alike {index:02}")
            },
            "reason": "reviewed_distinct"
        }));
    }
    fs::create_dir_all(dir.join("_escrow")).unwrap();
    fs::write(
        dir.join("_escrow/cannot_link.jsonl"),
        distinct
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n",
    )
    .unwrap();
}

fn card(path: &Path, id: &str) -> Value {
    serde_json::from_slice(&fs::read(path.join(format!("{id}.json"))).unwrap()).unwrap()
}

#[test]
fn entity_card_export_contains_context_retirement_keys_and_is_repeatable() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("registry");
    let first_out = temp.path().join("cards-one");
    let second_out = temp.path().join("cards-two");
    seed_registry(&registry, false);

    for out in [&first_out, &second_out] {
        canon()
            .args([
                "registry",
                "export",
                "--registry",
                registry.to_str().unwrap(),
                "--format",
                "entity-card",
                "--out",
                out.to_str().unwrap(),
            ])
            .assert()
            .success();
    }
    assert_eq!(
        fs::read(first_out.join("ORG-001.json")).unwrap(),
        fs::read(second_out.join("ORG-001.json")).unwrap()
    );

    let alpha = card(&first_out, "ORG-001");
    assert_eq!(alpha["version"], "canon.entity_card.v0");
    assert_eq!(alpha["agent_id"], "org_agent_alpha");
    assert_eq!(alpha["keys"]["cik"], json!(["111"]));
    assert_eq!(alpha["evidence"]["ref"], "cmdrvl://session/alpha");
    assert_eq!(alpha["units"][0]["id"], "UNIT-001");
    assert!(
        alpha["distinct"]
            .as_array()
            .unwrap()
            .iter()
            .any(|counterpart| counterpart["id"] == "ORG-002")
    );
    assert_eq!(card(&first_out, "UNIT-001")["part_of"][0]["id"], "ORG-001");

    let retired = card(&first_out, "ORG-009");
    assert_eq!(retired["status"], "retired");
    assert_eq!(retired["successor"], "ORG-001");

    let selected = temp.path().join("selected");
    canon()
        .args([
            "registry",
            "export",
            "--registry",
            registry.to_str().unwrap(),
            "--format",
            "entity-card",
            "--out",
            selected.to_str().unwrap(),
            "--id",
            "ORG-001",
        ])
        .assert()
        .success();
    assert!(selected.join("ORG-001.json").is_file());
    assert!(!selected.join("ORG-002.json").exists());

    let sqlite = temp.path().join("search.sqlite");
    canon()
        .args([
            "registry",
            "export",
            "--registry",
            registry.to_str().unwrap(),
            "--format",
            "search-index",
            "--out",
            sqlite.to_str().unwrap(),
        ])
        .assert()
        .success();
    let conn = Connection::open(sqlite).unwrap();
    let mut statement = conn
        .prepare(
            "SELECT canonical_id, kind, counterpart_id, counterpart_name
             FROM entity_context ORDER BY canonical_id, kind, counterpart_id, counterpart_name",
        )
        .unwrap();
    let mut rows = statement.query([]).unwrap();
    let mut context = Vec::new();
    while let Some(row) = rows.next().unwrap() {
        context.push((
            row.get::<_, String>(0).unwrap(),
            row.get::<_, String>(1).unwrap(),
            row.get::<_, Option<String>>(2).unwrap(),
            row.get::<_, String>(3).unwrap(),
        ));
    }
    let mut expected = BTreeSet::new();
    for (id, kind, field) in [
        ("ORG-001", "part_of", "part_of"),
        ("ORG-001", "unit", "units"),
        ("ORG-001", "distinct", "distinct"),
        ("ORG-002", "distinct", "distinct"),
        ("UNIT-001", "part_of", "part_of"),
    ] {
        let current = card(&first_out, id);
        for counterpart in current[field].as_array().unwrap() {
            expected.insert((
                id.to_string(),
                kind.to_string(),
                counterpart["id"].as_str().map(str::to_string),
                counterpart["name"].as_str().unwrap().to_string(),
            ));
        }
    }
    assert_eq!(context.into_iter().collect::<BTreeSet<_>>(), expected);
}

#[test]
fn entity_card_budget_truncates_in_order_and_refuses_below_floor() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("registry");
    seed_registry(&registry, true);
    let budgeted = temp.path().join("budgeted");
    canon()
        .args([
            "registry",
            "export",
            "--registry",
            registry.to_str().unwrap(),
            "--format",
            "entity-card",
            "--out",
            budgeted.to_str().unwrap(),
            "--id",
            "ORG-001",
            "--max-bytes",
            "700",
        ])
        .assert()
        .success();
    let bytes = fs::read(budgeted.join("ORG-001.json")).unwrap();
    assert!(bytes.len() <= 700, "card was {} bytes", bytes.len());
    let card: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(card["id"], "ORG-001");
    assert_eq!(card["kind"], "firm");
    assert_eq!(card["keys"]["cik"], json!(["111"]));
    assert!(!card["truncated"].as_array().unwrap().is_empty());
    assert!(
        card["truncated"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| { row["field"] == "distinct" && row["kept"] == 5 })
    );

    let refused_out = temp.path().join("refused");
    let refused = canon()
        .args([
            "registry",
            "export",
            "--registry",
            registry.to_str().unwrap(),
            "--format",
            "entity-card",
            "--out",
            refused_out.to_str().unwrap(),
            "--id",
            "ORG-001",
            "--max-bytes",
            "1",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refused).unwrap();
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert!(
        refusal["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("minimum")
    );
    assert!(!refused_out.join("ORG-001.json").exists());
}
