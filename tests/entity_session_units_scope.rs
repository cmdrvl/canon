//! bd-vdcw.54: a unit inside a firm is its own entity, `part_of` the firm; a
//! role-scoped spelling resolves to the unit and the same spelling unscoped to
//! the firm. Every flow runs the public binary on invented fixtures.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const FIRM: &str = "Harbor Trust Bank, National Association";
const UNIT: &str = "Harbor Servicing, a division of Harbor Trust Bank, National Association";
const CIK: &str = "0000111111";
const CIK_KEY: &str = "111111";

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
incumbent_attach:
  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]
"#;

const PROFILE: &str = r#"profile: units_scope_test
version: 0.1.0
entity_type: organisation
identity_semantics: same_normalized_name
canonical_type: org
required_fields: [source_row_id, surface]
normalized_views:
  core:
    operators: [legal_basename]
  row_key:
    operators: [ascii_trim_upper]
  structure:
    operators: [name_structure]
prepare:
  primary_surface_fields: [surface]
  canonical_surface_normalized_view: core
  normalized_view_fields:
    core: surface
    row_key: source_row_id
    structure: surface
  anchor_fields:
    cik: cik
  surface_key_fields: [row_key]
  nullable_fields: [cik]
  provenance_fields: [source_row_id, origin]
blocking:
  operators:
    - op: exact_identity
      view: core
evidence:
  support:
    - op: exact_view
      view: core
  cannot_link:
    - op: protected_token_conflict
      view: core
      params:
        score_units: "0"
  relation_hints:
    - op: cross_profile_alignment
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: units_scope_test.aliases
  distinct: units_scope_test.distinct
  relations: units_scope_test.relations
solve:
  hard_cannot_link: contradiction
  standalone_surfaces: include
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

fn p(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

struct Case {
    _temp: TempDir,
    root: PathBuf,
    registry: PathBuf,
    session: PathBuf,
    rules: PathBuf,
}

fn observation(
    obs_id: &str,
    lineage: &str,
    surface: &str,
    keys: Value,
    role: Option<&str>,
) -> Value {
    let mut row = json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": keys,
        "source": {"channel": "file", "id": "units-fixture", "lineage": lineage}
    });
    if let Some(role) = role {
        row["context"] = json!({"role": role});
    }
    row
}

/// The staged organisations registry: an invented firm and its unit as two
/// ids with nothing joining them, the role policy opted in, a session bound.
fn case(opt_in: bool) -> Case {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    let registry = root.join("organisations");
    run(
        &[
            "registry",
            "init",
            "--registry",
            p(&registry),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ],
        0,
    );
    for (id, name, rule) in [("ORG-012", FIRM, "FIRM"), ("ORG-053", UNIT, "UNIT")] {
        run(
            &[
                "registry",
                "mint",
                "--registry",
                p(&registry),
                "--canonical-id",
                id,
                "--canonical-type",
                "firm",
                "--with-alias",
                &format!("00-firms.json={name}:{rule}"),
                "--create-alias-file",
                "--bump",
                "patch",
                "--no-lint",
            ],
            0,
        );
    }
    if opt_in {
        run(
            &[
                "registry",
                "scope-policy",
                "--registry",
                p(&registry),
                "--optional-dimension",
                "role",
                "--bump",
                "patch",
            ],
            0,
        );
    }
    let rules = root.join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    let map = root.join("registries.yaml");
    fs::write(
        &map,
        format!(
            "version: canon.session.registries.v1\nkinds:\n  organisation:\n    registry: {}\n    registry_id: organisations\n    prefix: ORG\n    zero_pad: 3\n    canonical_type: firm\n    alias_file: 30-session-aliases.json\n",
            registry.display()
        ),
    )
    .unwrap();
    let session = root.join("session");
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "units",
            "--work-dir",
            p(&session),
            "--registries",
            p(&map),
        ],
        0,
    );
    Case {
        _temp: temp,
        root,
        registry,
        session,
        rules,
    }
}

impl Case {
    fn ingest(&self, rows: &[Value]) {
        let path = self.root.join("obs.jsonl");
        let text = rows
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{text}\n")).unwrap();
        run_json(
            &["entity", "session", "ingest", p(&self.session), p(&path)],
            0,
        );
    }

    fn derive(&self) -> Value {
        let profile = self.root.join("profile.yaml");
        fs::write(&profile, PROFILE).unwrap();
        let strategy = self.root.join("strategy.yaml");
        fs::write(
            &strategy,
            "block:\n  preflight: false\n  max_candidates_per_run: 5000000\n",
        )
        .unwrap();
        run_json(
            &[
                "entity",
                "session",
                "derive",
                p(&self.session),
                "--profile",
                p(&profile),
                "--strategy",
                p(&strategy),
            ],
            0,
        )
    }

    fn assert_lines(&self, name: &str, lines: &[Value]) {
        let path = self.root.join(name);
        let text = lines
            .iter()
            .map(|line| serde_json::to_string(line).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{text}\n")).unwrap();
        run_json(
            &["entity", "session", "assert", p(&self.session), p(&path)],
            0,
        );
    }

    fn solve(&self) -> Value {
        run_json(
            &[
                "entity",
                "session",
                "solve",
                p(&self.session),
                "--rules",
                p(&self.rules),
            ],
            0,
        )
    }

    fn commit(&self, version: &str) -> Value {
        run_json(
            &[
                "entity",
                "session",
                "commit",
                p(&self.session),
                "--next-version",
                version,
            ],
            0,
        )
    }

    /// The id a name resolves to in the registry, bare or under a role scope.
    fn lookup(&self, name: &str, scope: Option<&str>) -> Option<String> {
        let input = self.root.join("lookup.csv");
        fs::write(&input, format!("name\n\"{name}\"\n")).unwrap();
        let mut command = canon();
        command
            .arg(&input)
            .arg("--registry")
            .arg(&self.registry)
            .arg("--column")
            .arg("name")
            .args(["--emit", "json", "--explicit", "--plain-json-values"]);
        if let Some(scope) = scope {
            command.arg("--scope").arg(scope);
        }
        let output = command.output().unwrap();
        let payload: Value = serde_json::from_slice(&output.stdout).expect("lookup json");
        payload["mappings"][0]["canonical_id"]
            .as_str()
            .map(str::to_string)
    }

    fn registry_text(&self) -> String {
        let mut text = String::new();
        let mut files = fs::read_dir(&self.registry)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        files.sort();
        for file in files {
            if file.is_file() {
                text.push_str(&fs::read_to_string(&file).unwrap_or_default());
                text.push('\n');
            }
        }
        text
    }
}

fn scoped_alias_assertion(subject: &str, unit: &str) -> Value {
    json!({
        "type": "asserted_alias",
        "left": subject,
        "right": unit,
        "basis": {
            "scope": {"role": "servicer"},
            "subject": subject,
            "explanation": "the filing names the servicing division by the bank's name on the servicer line"
        },
        "source": {
            "channel": "filing",
            "id": "units-fixture-filing",
            "locator": "filing:units-fixture-filing#servicer-line",
            "content_hash": format!("blake3:{}", "a".repeat(64))
        },
        "asserted_by": "human:analyst"
    })
}

fn rows() -> Vec<Value> {
    vec![
        observation("u1", "fundA", UNIT, json!({}), None),
        observation("f1", "fundB", FIRM, json!({"cik": CIK}), Some("servicer")),
        observation("f2", "fundC", FIRM, json!({"cik": CIK}), Some("servicer")),
    ]
}

#[test]
fn a_cited_scoped_alias_gives_the_unit_scoped_and_the_firm_bare_with_part_of() {
    let case = case(true);
    // Before the session: the bare name is the firm; the unit has its own id.
    assert_eq!(case.lookup(FIRM, None).as_deref(), Some("ORG-012"));
    assert_eq!(case.lookup(UNIT, None).as_deref(), Some("ORG-053"));
    case.ingest(&rows());
    let derived = case.derive();
    assert!(
        derived["structure"]["part_of_records"].as_u64().unwrap() >= 1,
        "the 'a division of' surface is related part_of the firm: {derived}"
    );
    case.assert_lines("scoped.jsonl", &[scoped_alias_assertion("f1", "u1")]);
    let solved = case.solve();
    let units = solved["units"].as_array().expect("units reported");
    assert_eq!(units.len(), 1, "{solved}");
    assert_eq!(units[0]["unit_surface"], UNIT);
    assert_eq!(units[0]["part_of_surface"], FIRM);
    assert_eq!(units[0]["canonical_id"], "ORG-053");
    assert_eq!(units[0]["evidence"], "surface_pattern");
    assert_eq!(solved["scoped_aliases"][0]["role"], "servicer");
    // The unit and the firm are not one cluster.
    for cluster in solved["clusters"].as_array().unwrap() {
        let surfaces = cluster["surfaces"].as_array().unwrap();
        assert!(
            !(surfaces.iter().any(|s| s == UNIT) && surfaces.iter().any(|s| s == FIRM)),
            "{solved}"
        );
    }

    let committed = case.commit("0.9.0");
    assert_eq!(committed["outcome"], "committed", "{committed}");

    assert_eq!(
        case.lookup(FIRM, Some("role=servicer")).as_deref(),
        Some("ORG-053"),
        "scoped to the unit"
    );
    assert_eq!(case.lookup(FIRM, None).as_deref(), Some("ORG-012"));
    assert_eq!(case.lookup(UNIT, None).as_deref(), Some("ORG-053"));
    // A role with no scoped entry falls back to the firm.
    assert_eq!(
        case.lookup(FIRM, Some("role=trustee")).as_deref(),
        Some("ORG-012")
    );
    // The CIK is a key alias of the firm only.
    assert_eq!(case.lookup(CIK_KEY, None).as_deref(), Some("ORG-012"));
    let text = case.registry_text();
    assert!(text.contains("part_of"), "relations hold part_of: {text}");
}

#[test]
fn the_same_rows_with_a_role_and_no_scoped_evidence_create_no_scoped_entry() {
    let case = case(true);
    case.ingest(&rows());
    case.derive();
    let solved = case.solve();
    assert!(solved.get("scoped_aliases").is_none(), "{solved}");
    case.commit("0.9.0");
    // Role alone scopes nothing: the bare name under any role is still the firm.
    assert_eq!(
        case.lookup(FIRM, Some("role=servicer")).as_deref(),
        Some("ORG-012")
    );
    assert!(
        !case.registry_text().contains("\"binding\""),
        "no scoped entry was written"
    );
}

#[test]
fn look_alike_names_are_neither_unit_nor_alias() {
    let case = case(true);
    case.ingest(&[
        observation("b1", "fundA", "Black Rock Coffee Bar", json!({}), None),
        observation("b2", "fundB", "BlackRock, Inc.", json!({}), None),
    ]);
    let derived = case.derive();
    assert_eq!(derived["structure"]["part_of_records"], 0, "{derived}");
    let solved = case.solve();
    assert!(solved.get("units").is_none(), "{solved}");
    assert!(solved.get("scoped_aliases").is_none(), "{solved}");
    for cluster in solved["clusters"].as_array().unwrap() {
        assert_eq!(cluster["surfaces"].as_array().unwrap().len(), 1, "{solved}");
    }
}

#[test]
fn merge_evidence_after_commit_is_a_contradiction_not_a_merge() {
    let case = case(true);
    case.ingest(&rows());
    case.derive();
    case.assert_lines("scoped.jsonl", &[scoped_alias_assertion("f1", "u1")]);
    case.solve();
    case.commit("0.9.0");
    // Someone later asserts the unit and the firm are the same entity.
    case.assert_lines(
        "merge.jsonl",
        &[json!({
            "type": "asserted_alias",
            "left": "u1",
            "right": "f1",
            "basis": {"explanation": "a later claim that they are one"},
            "source": {
                "channel": "note", "id": "later-note",
                "locator": "note:later#1",
                "content_hash": format!("blake3:{}", "b".repeat(64))
            },
            "asserted_by": "human:analyst"
        })],
    );
    let solved = case.solve();
    let reasons = solved["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|cluster| {
            cluster["conflict_reasons"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .map(|reason| reason.as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert!(
        reasons.iter().any(|r| r == "related_not_same"),
        "{reasons:?} {solved}"
    );
}

#[test]
fn a_registry_that_has_not_opted_in_refuses_the_commit_of_a_scoped_alias() {
    let case = case(false);
    case.ingest(&rows());
    case.derive();
    case.assert_lines("scoped.jsonl", &[scoped_alias_assertion("f1", "u1")]);
    case.solve();
    let refused = run_json(
        &[
            "entity",
            "session",
            "commit",
            p(&case.session),
            "--next-version",
            "0.9.0",
        ],
        2,
    );
    assert_eq!(refused["outcome"], "REFUSAL", "{refused}");
    assert!(
        refused.to_string().contains("scope_policy_missing"),
        "{refused}"
    );
    // Nothing was written: no scoped entry, the bare lookup is unchanged.
    assert!(!case.registry_text().contains("\"binding\""));
    assert_eq!(case.lookup(FIRM, None).as_deref(), Some("ORG-012"));
}
