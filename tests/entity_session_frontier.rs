//! Public-command acceptance for `canon entity session frontier` and
//! `canon entity session next` (bd-vdcw.23): what the session still leaves
//! unresolved, as data, with the evidence kinds that would settle each part.
//! Every flow runs the real binary on temp sessions.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
  lei: {authoritative: true, single_valued: true}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
"#;

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn run(args: &[&str], expected_exit: i32) -> std::process::Output {
    let output = canon().args(args).output().expect("canon runs");
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "args={args:?} stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_json(args: &[&str], expected_exit: i32) -> Value {
    serde_json::from_slice(&run(args, expected_exit).stdout).expect("JSON stdout")
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

fn init(temp: &TempDir, label: &str) -> (PathBuf, PathBuf) {
    let session = temp.path().join(format!("{label}-session"));
    let rules = temp.path().join(format!("{label}-rules.yaml"));
    fs::write(&rules, RULES).unwrap();
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "frontier acceptance",
            "--registry",
            path_str(&temp.path().join(format!("{label}-registry"))),
            "--work-dir",
            path_str(&session),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "4",
        ],
        0,
    );
    (session, rules)
}

fn write_jsonl(temp: &TempDir, name: &str, rows: &[Value]) -> PathBuf {
    let path = temp.path().join(name);
    let text = rows
        .iter()
        .map(|row| serde_json::to_string(row).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, format!("{text}\n")).unwrap();
    path
}

fn observation(obs_id: &str, lineage: &str, surface: &str, keys: Value) -> Value {
    json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": keys,
        "source": {"channel": "fixture", "id": "frontier", "lineage": lineage}
    })
}

fn evidence(kind: &str, left: &str, right: &str, lineage: &str) -> Value {
    json!({
        "type": kind,
        "left": left,
        "right": right,
        "basis": {
            "rule": "fixture",
            "left_value": left,
            "right_value": right,
            "steps": ["fixture"]
        },
        "source": {"channel": "agent", "id": format!("{kind}-{lineage}"), "lineage": lineage},
        "asserted_by": "agent:fixture"
    })
}

fn ingest(session: &Path, file: &Path) {
    run_json(
        &[
            "entity",
            "session",
            "ingest",
            path_str(session),
            path_str(file),
        ],
        0,
    );
}

fn assert_evidence(session: &Path, file: &Path) {
    run_json(
        &[
            "entity",
            "session",
            "assert",
            path_str(session),
            path_str(file),
        ],
        0,
    );
}

fn solve(session: &Path, rules: &Path) -> Value {
    run_json(
        &[
            "entity",
            "session",
            "solve",
            path_str(session),
            "--rules",
            path_str(rules),
        ],
        0,
    )
}

fn frontier(session: &Path, extra: &[&str]) -> Value {
    let mut args = vec!["entity", "session", "frontier", path_str(session)];
    args.extend_from_slice(extra);
    run_json(&args, 0)
}

fn group<'a>(frontier: &'a Value, id: &str) -> Option<&'a Value> {
    frontier["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["group_id"] == id)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

/// 20 pairs missing an independent origin, 7 missing a second evidence type,
/// 3 contradictions (key conflicts), plus one lone keyed surface.
fn thirty_item_fixture(temp: &TempDir) -> (PathBuf, PathBuf, Vec<Value>, Vec<Value>) {
    let (session, rules) = init(temp, "thirty");
    let mut observations = Vec::new();
    let mut proof = Vec::new();
    for index in 0..20 {
        let (left, right) = (format!("P{index:02}a"), format!("P{index:02}b"));
        observations.push(observation(
            &left,
            "fundA",
            &format!("Origin Left {index}"),
            json!({}),
        ));
        observations.push(observation(
            &right,
            "fundA",
            &format!("Origin Right {index}"),
            json!({}),
        ));
        proof.push(evidence("same_slot", &left, &right, "fundA"));
        proof.push(evidence("name_equivalent", &left, &right, "fundA"));
    }
    for index in 0..7 {
        let (left, right) = (format!("Q{index:02}a"), format!("Q{index:02}b"));
        observations.push(observation(
            &left,
            "fundA",
            &format!("Kind Left {index}"),
            json!({}),
        ));
        observations.push(observation(
            &right,
            "fundB",
            &format!("Kind Right {index}"),
            json!({}),
        ));
        proof.push(evidence("same_slot", &left, &right, "fundA"));
    }
    for index in 0..3 {
        let (left, right) = (format!("C{index:02}a"), format!("C{index:02}b"));
        observations.push(observation(
            &left,
            "fundA",
            &format!("Clash Left {index}"),
            json!({"cik": format!("{}1", index + 1)}),
        ));
        observations.push(observation(
            &right,
            "fundB",
            &format!("Clash Right {index}"),
            json!({"cik": format!("{}2", index + 1)}),
        ));
        proof.push(evidence("same_slot", &left, &right, "fundA"));
        proof.push(evidence("name_equivalent", &left, &right, "fundB"));
    }
    observations.push(observation(
        "S1",
        "fundA",
        "Lone Keyed",
        json!({"cik": "999"}),
    ));
    ingest(
        &session,
        &write_jsonl(temp, "thirty-obs.jsonl", &observations),
    );
    assert_evidence(&session, &write_jsonl(temp, "thirty-ev.jsonl", &proof));
    solve(&session, &rules);
    (session, rules, observations, proof)
}

#[test]
fn groups_counts_coverage_and_the_supply_and_resolve_loop() {
    let temp = TempDir::new().unwrap();
    let (session, rules, observations, proof) = thirty_item_fixture(&temp);

    let before = frontier(&session, &[]);
    eprintln!("frontier before: {before}");
    assert_eq!(before["version"], "canon.entity.session.frontier.v1");
    assert_eq!(before["components"].as_array().unwrap().len(), 3);
    let origin = group(&before, "needs_independent_origin").expect("origin group");
    let kind = group(&before, "needs_second_evidence_type").expect("kind group");
    let human = group(&before, "needs_human").expect("human group");
    assert_eq!(origin["items"], 20);
    assert_eq!(kind["items"], 7);
    assert_eq!(human["items"], 3);
    assert_eq!(origin["missing"], "needs_independent_origin");
    assert_eq!(
        strings(&origin["settles_with"]),
        ["attribute_agreement", "name_equivalent", "same_slot"]
    );
    assert_eq!(
        strings(&kind["settles_with"]),
        ["attribute_agreement", "name_equivalent"]
    );
    assert!(human["settles_with"].as_array().unwrap().is_empty());
    assert_eq!(origin["surfaces"].as_array().unwrap().len(), 40);
    assert_eq!(origin["observation_count"], 40);

    // Coverage for key family cik equals counts made independently here from
    // the rows: attempted = distinct surfaces carrying a cik, per source; the
    // six clashing surfaces are conflicted, the lone keyed one is unresolved.
    let mut independent = BTreeMap::<String, std::collections::BTreeSet<String>>::new();
    for row in &observations {
        if row["keys"].get("cik").is_some() {
            independent
                .entry(row["source"]["lineage"].as_str().unwrap().to_string())
                .or_default()
                .insert(row["surface"].as_str().unwrap().to_string());
        }
    }
    let cik_rows = before["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["key_family"] == "cik")
        .collect::<Vec<_>>();
    assert_eq!(cik_rows.len(), independent.len());
    for row in cik_rows {
        let expected = independent[row["source"].as_str().unwrap()].len() as u64;
        assert_eq!(row["attempted"], expected, "{row}");
        assert_eq!(
            row["supported"].as_u64().unwrap()
                + row["conflicted"].as_u64().unwrap()
                + row["unresolved"].as_u64().unwrap(),
            expected
        );
    }
    let fund_a = before["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key_family"] == "cik" && row["source"] == "fundA")
        .unwrap();
    assert_eq!(fund_a["attempted"], 4);
    assert_eq!(fund_a["conflicted"], 3);
    // The lone keyed surface mints on its authoritative key: supported.
    assert_eq!(fund_a["supported"], 1);
    assert_eq!(fund_a["unresolved"], 0);
    let listed = before["unresolved_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key["value"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(listed.contains(&"999".to_string()), "{listed:?}");
    assert!(
        listed.contains(&"11".to_string()) && listed.contains(&"12".to_string()),
        "key values in a contradiction are listed: {listed:?}"
    );

    // No action language anywhere in the JSON.
    let text = before.to_string();
    for forbidden in ["\"how\"", "\"command\"", "\"recipe\"", "next_command"] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }

    // `next` prints the same groups and counts and nothing that is not in the
    // JSON: parse the summary back against the JSON.
    let next = String::from_utf8(run(&["entity", "session", "next", path_str(&session)], 0).stdout)
        .unwrap();
    let alias = String::from_utf8(
        run(
            &[
                "entity",
                "session",
                "frontier",
                path_str(&session),
                "--emit",
                "summary",
            ],
            0,
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(next, alias, "next is frontier --emit summary");
    let mut group_lines = BTreeMap::new();
    for line in next.lines() {
        if let Some(rest) = line.strip_prefix("group ") {
            let fields = rest
                .split_whitespace()
                .filter_map(|field| field.split_once('='))
                .collect::<BTreeMap<_, _>>();
            group_lines.insert(fields["id"].to_string(), fields.clone());
            let json_group = group(&before, fields["id"]).expect("summary group is in the JSON");
            assert_eq!(fields["items"], json_group["items"].to_string());
            assert_eq!(
                fields["surfaces"],
                json_group["surfaces"].as_array().unwrap().len().to_string()
            );
            assert_eq!(
                fields["observations"],
                json_group["observation_count"].to_string()
            );
            assert_eq!(
                fields["settles_with"],
                strings(&json_group["settles_with"]).join(",")
            );
        }
    }
    assert_eq!(group_lines.len(), 3, "{next}");
    assert!(next.contains(&format!(
        "solve_hash={}",
        before["solve_hash"].as_str().unwrap()
    )));

    // Shuffled input order: byte-identical JSON.
    let shuffled = TempDir::new().unwrap();
    let (shuffled_session, shuffled_rules) = init(&shuffled, "thirty");
    let mut reversed = observations.clone();
    reversed.reverse();
    ingest(
        &shuffled_session,
        &write_jsonl(&shuffled, "obs.jsonl", &reversed),
    );
    let mut reversed_proof = proof.clone();
    reversed_proof.reverse();
    assert_evidence(
        &shuffled_session,
        &write_jsonl(&shuffled, "ev.jsonl", &reversed_proof),
    );
    solve(&shuffled_session, &shuffled_rules);
    let mut lhs = before.clone();
    let mut rhs = frontier(&shuffled_session, &[]);
    // solve_hash covers the recorded generation ids, which differ per directory;
    // everything the frontier says about the session must be identical.
    lhs["solve_hash"] = Value::Null;
    rhs["solve_hash"] = Value::Null;
    assert_eq!(
        serde_json::to_string(&lhs).unwrap(),
        serde_json::to_string(&rhs).unwrap()
    );

    // Supply evidence for the 20 from a new origin and re-solve: the group goes.
    let supplied = (0..20)
        .map(|index| {
            evidence(
                "name_equivalent",
                &format!("P{index:02}a"),
                &format!("P{index:02}b"),
                "fundC",
            )
        })
        .collect::<Vec<_>>();
    assert_evidence(&session, &write_jsonl(&temp, "supply.jsonl", &supplied));
    // Before the re-solve the old solve is stale: a stale solve refuses.
    let stale = run_json(&["entity", "session", "frontier", path_str(&session)], 2);
    assert_eq!(stale["refusal"]["detail"]["reason"], "stale_solve");
    solve(&session, &rules);
    let after = frontier(&session, &[]);
    eprintln!("frontier after: {after}");
    assert!(
        group(&after, "needs_independent_origin").is_none(),
        "the supplied origin settled the group"
    );
    assert_eq!(
        group(&after, "needs_second_evidence_type").unwrap()["items"],
        7
    );
    assert_eq!(group(&after, "needs_human").unwrap()["items"], 3);
}

#[test]
fn overlapping_origins_count_as_one_and_independent_ones_do_not() {
    let temp = TempDir::new().unwrap();
    let (session, rules) = init(&temp, "origins");
    let with_ancestry = |obs_id: &str, surface: &str, origins: &[&str]| {
        json!({
            "obs_id": obs_id,
            "entity_type": "organisation",
            "surface": surface,
            "keys": {},
            "source": {
                "channel": "fixture",
                "id": "frontier",
                "ancestry": {"status": "known", "origin_source_ids": origins}
            }
        })
    };
    ingest(
        &session,
        &write_jsonl(
            &temp,
            "origins-obs.jsonl",
            &[
                // Share origin "y": one origin component.
                with_ancestry("o1", "Overlap Left", &["x", "y"]),
                with_ancestry("o2", "Overlap Right", &["y", "z"]),
                // Disjoint origins: two independent components.
                with_ancestry("d1", "Disjoint Left", &["p"]),
                with_ancestry("d2", "Disjoint Right", &["q"]),
            ],
        ),
    );
    assert_evidence(
        &session,
        &write_jsonl(
            &temp,
            "origins-ev.jsonl",
            &[
                evidence("same_slot", "o1", "o2", "x"),
                evidence("name_equivalent", "o1", "o2", "z"),
                evidence("same_slot", "d1", "d2", "p"),
                evidence("name_equivalent", "d1", "d2", "q"),
            ],
        ),
    );
    solve(&session, &rules);
    let result = frontier(&session, &[]);
    eprintln!("origins frontier: {result}");
    let origin = group(&result, "needs_independent_origin").expect("overlap is listed");
    assert_eq!(origin["items"], 1);
    let surfaces = strings(&origin["surfaces"]);
    assert_eq!(surfaces, ["Overlap Left", "Overlap Right"]);
    assert!(
        !surfaces
            .iter()
            .any(|surface| surface.starts_with("Disjoint")),
        "an item already at two independent origins is not listed"
    );
}

#[test]
fn history_requests_and_the_seed_neighbourhood() {
    let temp = TempDir::new().unwrap();
    let (session, rules) = init(&temp, "history");
    let row = |obs_id: &str, lineage: &str, surface: &str, context: Value, keys: Value| {
        json!({
            "obs_id": obs_id,
            "entity_type": "organisation",
            "surface": surface,
            "keys": keys,
            "context": context,
            "source": {"channel": "fixture", "id": "frontier", "lineage": lineage}
        })
    };
    ingest(
        &session,
        &write_jsonl(
            &temp,
            "history-obs.jsonl",
            &[
                row(
                    "h1",
                    "fundA",
                    "Beacon Sponsor LLC",
                    json!({"deal": "D1", "loan": "L7", "role": "sponsor", "as_of_date": "2025-06-30"}),
                    json!({"cik": "500"}),
                ),
                row(
                    "h2",
                    "fundB",
                    "Beacon Sponsor",
                    json!({"deal": "D1", "loan": "L7", "role": "sponsor", "as_of_date": "2025-03-31"}),
                    json!({}),
                ),
                // A unit of the firm "Beacon Sponsor LLC": role-marked, no part_of record.
                row(
                    "u1",
                    "fundA",
                    "Beacon Sponsor LLC Fund II",
                    json!({"role": "fund", "parent": "Beacon Sponsor LLC"}),
                    json!({"cik": "501"}),
                ),
                // An unrelated look-alike sharing a token, with its own review item.
                row("l1", "fundA", "Beacon Hill Partners", json!({}), json!({})),
                row(
                    "l2",
                    "fundB",
                    "Beacon Hill Partners LP",
                    json!({}),
                    json!({}),
                ),
            ],
        ),
    );
    assert_evidence(
        &session,
        &write_jsonl(
            &temp,
            "history-ev.jsonl",
            &[
                evidence("name_equivalent", "h1", "h2", "fundA"),
                evidence("name_equivalent", "l1", "l2", "fundA"),
            ],
        ),
    );
    solve(&session, &rules);
    let before = frontier(&session, &[]);
    eprintln!("history frontier before: {before}");
    let requests = before["history_requests"].as_array().unwrap();
    let sponsor = requests
        .iter()
        .find(|request| request["scope"]["role"] == "sponsor")
        .expect("history request for the sponsor pair");
    assert_eq!(sponsor["need"], "prior_observations");
    assert_eq!(sponsor["scope"]["deal"], "D1");
    assert_eq!(sponsor["scope"]["loan"], "L7");
    assert_eq!(sponsor["before"], "2025-03-31");
    assert_eq!(strings(&sponsor["settles_with"]), ["same_slot"]);

    // Seeded by the firm: its missing LEI, the unit's missing part_of
    // evidence, and the sponsor pair, but not the look-alike's own review item.
    let seeded = frontier(&session, &["--seed-entity", "Beacon Sponsor LLC"]);
    eprintln!("history frontier seeded: {seeded}");
    let seed = &seeded["seed"];
    let missing_keys = seed["unresolved_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| {
            (
                key["entity"].as_str().unwrap().to_string(),
                key["namespace"].as_str().unwrap().to_string(),
            )
        })
        .collect::<Vec<_>>();
    assert!(missing_keys.contains(&("Beacon Sponsor LLC".to_string(), "lei".to_string())));
    assert!(missing_keys.contains(&("Beacon Sponsor LLC Fund II".to_string(), "lei".to_string())));
    let units = seed["unresolved_units"].as_array().unwrap();
    assert_eq!(units.len(), 1, "{units:?}");
    assert_eq!(units[0]["surface"], "Beacon Sponsor LLC Fund II");
    assert_eq!(strings(&units[0]["settles_with"]), ["part_of"]);
    let seeded_text = seeded.to_string();
    assert!(
        !seeded_text.contains("Beacon Hill"),
        "the look-alike's own review items stay out of the seed's frontier"
    );
    assert!(
        seeded["history_requests"].as_array().unwrap().len() == 1,
        "only the seed's own history is requested"
    );
    // Without a seed the look-alike review item is in the frontier.
    assert!(before.to_string().contains("Beacon Hill Partners"));

    // An unknown seed refuses.
    let missing = run_json(
        &[
            "entity",
            "session",
            "frontier",
            path_str(&session),
            "--seed-entity",
            "Nobody",
        ],
        2,
    );
    assert!(
        missing["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("Nobody")
    );

    // Supply the earlier same_slot evidence and re-solve: the request goes.
    assert_evidence(
        &session,
        &write_jsonl(
            &temp,
            "history-more.jsonl",
            &[evidence("same_slot", "h1", "h2", "fundB")],
        ),
    );
    solve(&session, &rules);
    let after = frontier(&session, &[]);
    eprintln!("history frontier after: {after}");
    assert!(
        after["history_requests"]
            .as_array()
            .unwrap()
            .iter()
            .all(|request| request["scope"]["role"] != "sponsor"),
        "same_slot evidence removed the request"
    );
}
