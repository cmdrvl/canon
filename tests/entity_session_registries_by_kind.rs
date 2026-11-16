#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path, path::PathBuf};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  authoritative: [cik]
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

fn json_stdout(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).expect("JSON stdout")
}

fn write_lines(temp: &Path, name: &str, lines: &[String]) -> PathBuf {
    let path = temp.join(name);
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("fixture");
    path
}

fn observation(obs_id: &str, entity_type: &str, surface: &str, cik: Option<&str>) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": entity_type,
        "surface": surface,
        "keys": {"cik": cik},
        "source": {"channel": "file", "id": "per-kind", "lineage": obs_id}
    }))
    .expect("observation JSON")
}

fn snapshot_files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries = fs::read_dir(current)
            .expect("read registry")
            .map(|entry| entry.expect("read registry entry"))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.push((
                    path.strip_prefix(root)
                        .expect("relative path")
                        .to_path_buf(),
                    fs::read(path).expect("read registry file"),
                ));
            }
        }
    }

    let mut files = Vec::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn session_registries_keep_kinds_separate_and_reject_unmapped_rows() {
    let temp = TempDir::new().expect("temp root");
    let organisations = temp.path().join("organisations");
    let people = temp.path().join("people");
    let session = temp.path().join("session");
    let rules = write_lines(temp.path(), "rules.yaml", &[RULES.trim_end().to_string()]);

    canon()
        .args([
            "registry",
            "init",
            "--registry",
            organisations.to_str().expect("organisations path"),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ])
        .assert()
        .success();
    let original_organisation_files = snapshot_files(&organisations);
    let map = write_lines(
        temp.path(),
        "registries.yaml",
        &[format!(
            "version: canon.session.registries.v1\nkinds:\n  organisation:\n    registry: {}\n    registry_id: organisations\n    prefix: ORG\n    zero_pad: 3\n    canonical_type: firm\n    alias_file: 30-session-aliases.json\n  person:\n    registry: {}\n    registry_id: people\n    id_scheme: agent\n    kind_prefix: per\n    canonical_type: person\n    alias_file: aliases.json",
            organisations.display(),
            people.display()
        )],
    );

    let init = canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "separate kinds",
            "--work-dir",
            session.to_str().expect("session path"),
            "--registries",
            map.to_str().expect("map path"),
        ])
        .assert()
        .success();
    let init_json = json_stdout(&init);
    assert_eq!(
        init_json["registries"]
            .as_array()
            .expect("registries")
            .len(),
        2
    );
    assert!(people.join("registry.json").is_file());
    assert!(organisations.join("30-session-aliases.json").is_file());

    let observations = write_lines(
        temp.path(),
        "observations.jsonl",
        &[
            observation("org-1", "organisation", "Shared Surface", Some("111")),
            observation("person-1", "person", "Shared Surface", Some("222")),
            observation("fund-1", "fund", "Unmapped Fund", None),
        ],
    );
    let ingest = canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session path"),
            observations.to_str().expect("observations path"),
        ])
        .assert()
        .code(1);
    let ingest_json = json_stdout(&ingest);
    assert_eq!(ingest_json["accepted"], 2);
    assert_eq!(ingest_json["rejected"][0]["code"], "unmapped_entity_kind");

    canon()
        .args([
            "entity",
            "session",
            "solve",
            session.to_str().expect("session path"),
            "--rules",
            rules.to_str().expect("rules path"),
        ])
        .assert()
        .success();
    let commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().expect("session path"),
            "--next-version",
            "organisation=1.0.1,person=0.1.0",
        ])
        .assert()
        .success();
    let commit_json = json_stdout(&commit);
    assert_eq!(
        commit_json["registries"]
            .as_array()
            .expect("receipt registries")
            .len(),
        2
    );

    let after = snapshot_files(&organisations);
    for (relative, before) in original_organisation_files {
        if relative == Path::new("registry.json") {
            continue;
        }
        let current = after
            .iter()
            .find(|(path, _)| path == &relative)
            .map(|(_, bytes)| bytes)
            .expect("original registry file remains");
        assert_eq!(
            current,
            &before,
            "original file changed: {}",
            relative.display()
        );
    }
    assert!(organisations.join("30-session-aliases.json").is_file());
    assert!(people.join("aliases.json").is_file());

    for registry in [&organisations, &people] {
        canon()
            .args([
                "registry",
                "lint",
                registry.to_str().expect("registry path"),
                "--emit",
                "json",
            ])
            .assert()
            .success();
    }
}

const ALPHABET: &str = "0123456789abcdefghjkmnpqrstvwxyz";
const SAMPLE_ID: &str = "org_7k3m9q2xh43z";

fn check_id(id: &str, extra: &[&str]) -> (i32, Value) {
    let output = canon()
        .args(["registry", "check-id", id])
        .args(extra)
        .output()
        .expect("run check-id");
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "check-id stdout is JSON ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    (output.status.code().expect("exit code"), value)
}

fn assert_bad_id(id: &str, reason: &str, extra: &[&str]) {
    let (code, value) = check_id(id, extra);
    assert_eq!(code, 2, "{id} must be refused: {value}");
    assert_eq!(value["refusal"]["code"], "E_BAD_ID", "{id}: {value}");
    assert_eq!(
        value["refusal"]["detail"]["reason"], reason,
        "{id}: {value}"
    );
    assert!(
        value["refusal"]["next_command"].is_string(),
        "{id} refusal carries next_command"
    );
}

#[test]
fn check_id_refuses_every_single_symbol_substitution_and_neighbour_swap() {
    let (code, good) = check_id(SAMPLE_ID, &[]);
    assert_eq!(code, 0, "{good}");
    assert_eq!(good["id"], SAMPLE_ID);
    assert_eq!(good["resolves_to"], Value::Null);
    assert_eq!(good["label"], "absent");

    let start = "org_".len();
    let mut checked = 0;
    for position in start..SAMPLE_ID.len() {
        for replacement in ALPHABET.chars() {
            if SAMPLE_ID.as_bytes()[position] as char == replacement {
                continue;
            }
            let mut mutated = SAMPLE_ID.to_string();
            mutated.replace_range(position..=position, &replacement.to_string());
            assert_bad_id(&mutated, "bad_check", &[]);
            checked += 1;
        }
    }
    for position in start..SAMPLE_ID.len() - 1 {
        let bytes = SAMPLE_ID.as_bytes();
        if bytes[position] == bytes[position + 1] {
            continue;
        }
        let mut swapped = bytes.to_vec();
        swapped.swap(position, position + 1);
        assert_bad_id(&String::from_utf8(swapped).unwrap(), "bad_check", &[]);
        checked += 1;
    }
    assert!(
        checked > 300,
        "expected the full enumeration, ran {checked}"
    );
}

#[test]
fn check_id_reports_each_refusal_reason_and_normalises_case_and_labels() {
    assert_bad_id("org_7k3m9q2xh4", "bad_length", &[]);
    assert_bad_id("org_7k3m9q2xh43zz", "bad_length", &[]);
    assert_bad_id("org_7k3m9q2xh4iz", "bad_symbol", &[]);
    assert_bad_id("org_7k3m9q2xh4uz", "bad_symbol", &[]);
    assert_bad_id("7k3m9q2xh43zabcd", "unknown_kind", &[]);

    let temp = TempDir::new().expect("temp root");
    let map = write_lines(
        temp.path(),
        "registries.yaml",
        &[format!(
            "version: canon.session.registries.v1\nkinds:\n  person:\n    registry: {}\n    registry_id: people\n    id_scheme: agent\n    kind_prefix: per\n    canonical_type: person\n    alias_file: aliases.json",
            temp.path().join("people").display()
        )],
    );
    // A kind the map does not declare is refused before any lookup.
    assert_bad_id(
        SAMPLE_ID,
        "unknown_kind",
        &["--registries", map.to_str().unwrap()],
    );

    let (code, upper) = check_id("ORG_7K3M9Q2XH43Z", &[]);
    assert_eq!(code, 0);
    assert_eq!(upper["id"], SAMPLE_ID, "upper case checks as lower case");

    let (code, labelled) = check_id("org_7k3m9q2xh43z~no-such-alias", &[]);
    assert_eq!(code, 0, "a label mismatch is never a refusal");
    assert_eq!(labelled["label"], "mismatch");
    assert!(
        labelled["resolves_to"].is_null(),
        "an id no registry holds resolves to nothing"
    );
}

struct Flow {
    organisations: PathBuf,
    map: PathBuf,
    commit: Value,
}

fn run_flow(root: &Path) -> Flow {
    fs::create_dir_all(root).expect("flow root");
    let organisations = root.join("organisations");
    let people = root.join("people");
    let session = root.join("session");
    let rules = write_lines(root, "rules.yaml", &[RULES.trim_end().to_string()]);
    canon()
        .args([
            "registry",
            "init",
            "--registry",
            organisations.to_str().unwrap(),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ])
        .assert()
        .success();
    let map = write_lines(
        root,
        "registries.yaml",
        &[format!(
            "version: canon.session.registries.v1\nkinds:\n  organisation:\n    registry: {}\n    registry_id: organisations\n    prefix: ORG\n    zero_pad: 3\n    agent_id: {{kind_prefix: org}}\n    canonical_type: firm\n    alias_file: 30-session-aliases.json\n    id_alias_file: 01-agent-ids.json\n  person:\n    registry: {}\n    registry_id: people\n    id_scheme: agent\n    kind_prefix: per\n    canonical_type: person\n    alias_file: aliases.json",
            organisations.display(),
            people.display()
        )],
    );
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "agent ids",
            "--work-dir",
            session.to_str().unwrap(),
            "--registries",
            map.to_str().unwrap(),
        ])
        .assert()
        .success();
    let observations = write_lines(
        root,
        "observations.jsonl",
        &[
            observation("org-1", "organisation", "Alpha Corp", Some("111")),
            observation("org-2", "organisation", "Beta Corp", Some("222")),
            observation("person-1", "person", "Alpha Corp", Some("333")),
        ],
    );
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            observations.to_str().unwrap(),
        ])
        .assert()
        .success();
    canon()
        .args([
            "entity",
            "session",
            "solve",
            session.to_str().unwrap(),
            "--rules",
            rules.to_str().unwrap(),
        ])
        .assert()
        .success();
    let commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().unwrap(),
            "--next-version",
            "organisation=1.0.0,person=0.1.0",
        ])
        .assert()
        .success();
    Flow {
        organisations,
        map,
        commit: json_stdout(&commit),
    }
}

/// (canonical id, agent id) for every minted entry that carries an agent id alias.
fn agent_id_pairs(commit: &Value, registry_kind_numbered: bool) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for minted in commit["minted"].as_array().expect("minted") {
        let canonical_id = minted["canonical_id"].as_str().expect("canonical_id");
        let numbered = canonical_id.starts_with("ORG-");
        if numbered != registry_kind_numbered {
            continue;
        }
        let agent_aliases = minted["aliases"]
            .as_array()
            .expect("aliases")
            .iter()
            .filter(|alias| alias["rule_id"] == "SESSION_AGENT_ID")
            .map(|alias| alias["input"].as_str().expect("input").to_string())
            .collect::<Vec<_>>();
        if numbered {
            assert_eq!(
                agent_aliases.len(),
                1,
                "{canonical_id} gets exactly one agent id alias: {minted}"
            );
            pairs.push((canonical_id.to_string(), agent_aliases[0].clone()));
        } else {
            assert!(agent_aliases.is_empty(), "{minted}");
            pairs.push((canonical_id.to_string(), canonical_id.to_string()));
        }
    }
    pairs.sort();
    pairs
}

#[test]
fn each_new_organisation_id_gets_one_agent_id_alias_that_resolves_by_plain_lookup() {
    let temp = TempDir::new().expect("temp root");
    let first = run_flow(&temp.path().join("first"));
    let pairs = agent_id_pairs(&first.commit, true);
    assert_eq!(
        pairs.len(),
        2,
        "two organisations were minted: {}",
        first.commit
    );
    for (canonical_id, agent_id) in &pairs {
        assert!(canonical_id.starts_with("ORG-"), "{canonical_id}");
        assert!(agent_id.starts_with("org_"), "{agent_id}");
        let (code, checked) = check_id(agent_id, &["--registries", first.map.to_str().unwrap()]);
        assert_eq!(code, 0, "{agent_id}: {checked}");
        assert_eq!(checked["kind"], "organisation");
        assert_eq!(checked["resolves_to"], canonical_id.as_str());

        // Plain exact lookup of the agent id returns the original ORG id.
        let rows = write_lines(
            temp.path(),
            "agent-id-rows.csv",
            &["name".to_string(), agent_id.clone()],
        );
        let lookup = canon()
            .args([
                rows.to_str().unwrap(),
                "--registry",
                first.organisations.to_str().unwrap(),
                "--column",
                "name",
                "--explicit",
                "--plain-json-values",
                "--no-witness",
            ])
            .assert()
            .success();
        let mapped = json_stdout(&lookup);
        assert_eq!(mapped["mappings"][0]["canonical_id"], canonical_id.as_str());
    }

    // Labels compare with the entity's aliases: slug the minted surface alias.
    let (first_org, first_agent) = &pairs[0];
    let surface = first.commit["minted"]
        .as_array()
        .expect("minted")
        .iter()
        .find(|minted| minted["canonical_id"] == first_org.as_str())
        .and_then(|minted| minted["aliases"].as_array())
        .and_then(|aliases| {
            aliases
                .iter()
                .filter_map(|alias| alias["input"].as_str())
                .find(|input| input.ends_with("Corp"))
        })
        .expect("a surface alias on the first organisation")
        .to_string();
    let slug = surface.to_lowercase().replace(' ', "-");
    let (code, matched) = check_id(
        &format!("{first_agent}~{slug}"),
        &["--registries", first.map.to_str().unwrap()],
    );
    assert_eq!(code, 0, "{matched}");
    assert_eq!(matched["resolves_to"], first_org.as_str());
    assert_eq!(matched["label"], "match");
    let (code, mismatched) = check_id(
        &format!("{first_agent}~not-this-entity"),
        &["--registries", first.map.to_str().unwrap()],
    );
    assert_eq!(code, 0);
    assert_eq!(mismatched["label"], "mismatch");

    // The person registry's canonical ids are agent ids that pass check-id.
    let people = agent_id_pairs(&first.commit, false);
    assert_eq!(people.len(), 1, "{}", first.commit);
    let (person_id, _) = &people[0];
    assert!(person_id.starts_with("per_"), "{person_id}");
    let (code, person) = check_id(person_id, &["--registries", first.map.to_str().unwrap()]);
    assert_eq!(code, 0, "{person}");
    assert_eq!(person["kind"], "person");
    assert_eq!(person["resolves_to"], person_id.as_str());

    // The same inputs into a second fresh pair of registries mint the same ids.
    let second = run_flow(&temp.path().join("second"));
    assert_eq!(agent_id_pairs(&second.commit, true), pairs);
    assert_eq!(agent_id_pairs(&second.commit, false), people);
}
