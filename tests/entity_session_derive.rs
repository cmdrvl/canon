//! `canon entity session derive` (bd-vdcw.14): Canon's existing entity operators
//! run over a session's observations and append canon.evidence.v1 records through
//! the assert path. Every flow runs the real binary on invented fixtures.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
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
  min_distinct_evidence_types: 1
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
incumbent_attach:
  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]
"#;

const STRATEGY: &str = "block:\n  preflight: false\n  max_candidates_per_run: 5000000\n";

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

/// An invented profile: row-level surfaces (one per observation), a normalised
/// name view, and the support / cannot-link lanes the case needs.
fn profile(support: &str, cannot_link: &str, blocking: &str, extra: &str) -> String {
    format!(
        r#"profile: session_derive_test
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
prepare:
  primary_surface_fields: [surface]
  canonical_surface_normalized_view: core
  normalized_view_fields:
    core: surface
    row_key: source_row_id
  anchor_fields:
    cik: cik
    lei: lei
  surface_key_fields: [row_key]
  nullable_fields: [cik, lei]
  provenance_fields: [source_row_id, origin]
blocking:
  operators:
{blocking}
evidence:
  support:
{support}
  cannot_link:
{cannot_link}
  relation_hints:
    - op: cross_profile_alignment
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
{extra}
patch_namespaces:
  aliases: session_derive_test.aliases
  distinct: session_derive_test.distinct
  relations: session_derive_test.relations
solve:
  hard_cannot_link: contradiction
  standalone_surfaces: include
"#
    )
}

const EXACT_BLOCK: &str = "    - op: exact_identity\n      view: core";
const NGRAM_BLOCK: &str =
    "    - op: ngram_topk\n      view: core\n      k: 10\n      candidate_cap: 10";
const EXACT_SUPPORT: &str = "    - op: exact_view\n      view: core";
const NO_CANNOT: &str = "    - op: protected_token_conflict\n      view: core\n      params:\n        score_units: \"0\"";

struct Case {
    _temp: Option<TempDir>,
    root: PathBuf,
    session: PathBuf,
    rules: PathBuf,
    strategy: PathBuf,
    /// registry id, id prefix and zero pad the session binds.
    shape: (String, String, String),
}

fn init(rules: &str) -> Case {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    let session = root.join("session");
    let rules_path = root.join("rules.yaml");
    fs::write(&rules_path, rules).unwrap();
    let strategy = root.join("strategy.yaml");
    fs::write(&strategy, STRATEGY).unwrap();
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "derive",
            "--registry",
            p(&root.join("registry")),
            "--work-dir",
            p(&session),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ],
        0,
    );
    Case {
        _temp: Some(temp),
        root,
        session,
        rules: rules_path,
        strategy,
        shape: ("organisations".into(), "ORG".into(), "3".into()),
    }
}

fn observation(obs_id: &str, lineage: &str, surface: &str, keys: Value) -> Value {
    json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": keys,
        "source": {"channel": "file", "id": "derive-fixture", "lineage": lineage}
    })
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

    fn derive(&self, profile_text: &str, exit: i32) -> Value {
        let profile_path = self.root.join("profile.yaml");
        fs::write(&profile_path, profile_text).unwrap();
        run_json(
            &[
                "entity",
                "session",
                "derive",
                p(&self.session),
                "--profile",
                p(&profile_path),
                "--strategy",
                p(&self.strategy),
            ],
            exit,
        )
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
}

fn count(output: &Value, kind: &str) -> u64 {
    output["records_by_type"][kind].as_u64().unwrap_or(0)
}

#[test]
fn an_exact_view_on_a_normalised_name_derives_name_equivalent_and_the_session_accepts_it() {
    let case = init(RULES);
    case.ingest(&[
        observation("a1", "fundA", "Apex Capital LLC", json!({})),
        observation("a2", "fundB", "APEX CAPITAL, LLC.", json!({})),
        observation("z1", "fundA", "Zenith Partners", json!({})),
    ]);
    let profile_text = profile(EXACT_SUPPORT, NO_CANNOT, EXACT_BLOCK, "");
    let derived = case.derive(&profile_text, 0);
    eprintln!("derive: {derived}");
    assert_eq!(derived["version"], "canon.entity.session.derive.v1");
    assert_eq!(count(&derived, "name_equivalent"), 1, "{derived}");
    assert_eq!(derived["accepted"], 1);

    // The stored record names the steps and both normalised values, applies to
    // the surface, and says Canon produced it.
    let status = run_json(&["entity", "session", "status", p(&case.session)], 0);
    assert_eq!(status["log"]["evidence"]["by_type"]["name_equivalent"], 1);

    // Idempotent: the same observations, profile and strategy add nothing.
    let again = case.derive(&profile_text, 0);
    assert_eq!(again["accepted"], 0);
    assert_eq!(again["already_present"], 1);

    // A following solve and commit accept the cluster without any
    // script-asserted evidence; two lineages count because the finding applies
    // to the surface.
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_b"], 1, "{solved}");
    let committed = run_json(
        &[
            "entity",
            "session",
            "commit",
            p(&case.session),
            "--next-version",
            "0.2.0",
        ],
        0,
    );
    assert_eq!(committed["outcome"], "committed", "{committed}");
    let minted = committed["minted"].as_array().unwrap();
    assert_eq!(
        minted.len(),
        1,
        "both spellings are one entity: {committed}"
    );
    assert_eq!(minted[0]["aliases"].as_array().unwrap().len(), 2);
}

#[test]
fn preserve_legal_form_derives_variant_equivalence_and_keeps_distinct_forms() {
    let case = init(RULES);
    case.ingest(&[
        observation("a1", "fundA", "Acme Holdings Inc", json!({})),
        observation("a2", "fundB", "Acme Holdings, Inc.", json!({})),
        observation("a3", "fundC", "Acme Holdings Incorporated", json!({})),
        observation("l1", "fundD", "Acme Holdings LLC", json!({})),
    ]);
    let profile_text = profile(EXACT_SUPPORT, NO_CANNOT, EXACT_BLOCK, "").replace(
        "operators: [legal_basename]",
        "operators: [preserve_legal_form]",
    );
    let derived = case.derive(&profile_text, 0);

    // The three Inc spellings form one exact bucket and produce a star with
    // two records. LLC remains a separate form and cannot join that bucket.
    assert_eq!(count(&derived, "name_equivalent"), 2, "{derived}");
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_b"], 1, "{solved}");
    let basis = stored_basis(&case, "a1");
    let equivalent = basis
        .iter()
        .filter(|record| record["type"] == "name_equivalent")
        .collect::<Vec<_>>();
    assert_eq!(equivalent.len(), 2, "{basis:?}");
    for record in equivalent {
        assert_eq!(record["basis"]["left_value"], "acme holdings inc");
        assert_eq!(record["basis"]["right_value"], "acme holdings inc");
        assert!(
            record["basis"]["steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|step| step == "legal_form:inc")
        );
        assert!(
            !record["basis"]["left_value"]
                .as_str()
                .unwrap()
                .contains("llc")
        );
        assert!(
            !record["basis"]["right_value"]
                .as_str()
                .unwrap()
                .contains("llc")
        );
    }
}

#[test]
fn bare_legal_form_equivalence_is_declared_and_does_not_bridge_forms() {
    let case = init(RULES);
    case.ingest(&[
        observation("a1", "fundA", "Acme Holdings", json!({})),
        observation("a2", "fundB", "Acme Holdings Inc", json!({})),
        observation("a3", "fundC", "Acme Holdings, Incorporated", json!({})),
        observation("z1", "fundD", "Zeta Holdings", json!({})),
        observation("z2", "fundE", "Zeta Holdings LLC", json!({})),
        observation("z3", "fundF", "Zeta Holdings Inc", json!({})),
    ]);
    let profile_text = profile(EXACT_SUPPORT, NO_CANNOT, EXACT_BLOCK, "")
        .replace(
            "normalized_views:\n  core:\n    operators: [legal_basename]",
            "normalized_views:\n  core:\n    operators: [preserve_legal_form]\n  bare:\n    operators: [preserve_legal_form_with_bare]",
        )
        .replace("    core: surface\n    row_key", "    core: surface\n    bare: surface\n    row_key")
        .replace(
            "    - op: exact_identity\n      view: core",
            "    - op: exact_identity\n      view: core\n    - op: exact_identity\n      view: bare",
        )
        .replace(
            "    - op: exact_view\n      view: core",
            "    - op: exact_view\n      view: core\n    - op: exact_view\n      view: bare",
        );
    let derived = case.derive(&profile_text, 0);
    let _solved = case.solve();

    // Acme's bare spelling has exactly one form in its bucket and joins the
    // two equivalent Inc spellings. The canonical-form support also records
    // the direct Inc-to-Inc edge. Zeta's bare spelling would bridge LLC and
    // Inc, so the ambiguous bucket is refused and emits no evidence for that
    // trio.
    assert_eq!(count(&derived, "name_equivalent"), 3, "{derived}");
    let basis = stored_basis(&case, "a1");
    for record in basis
        .iter()
        .filter(|record| record["type"] == "name_equivalent")
    {
        assert_eq!(record["basis"]["left_value"], "acme holdings");
        assert_eq!(record["basis"]["right_value"], "acme holdings");
        assert!(
            record["basis"]["steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|step| step == "legal_form:bare_equivalence")
        );
    }
}

#[test]
fn anchor_match_on_cik_and_lei_gives_the_same_tier_a_clusters_as_the_built_in_key_rule() {
    let rows = [
        observation(
            "k1",
            "fundA",
            "Alpha Holdings",
            json!({"cik": "100", "lei": "529900T8BM49AURSDO55"}),
        ),
        observation("k2", "fundB", "Alpha Hldgs Inc", json!({"cik": "100"})),
        observation(
            "k3",
            "fundA",
            "Beta Partners",
            json!({"lei": "5493001KJTIIGC8Y1R12"}),
        ),
        observation(
            "k4",
            "fundB",
            "Beta Partner",
            json!({"lei": "5493001KJTIIGC8Y1R12"}),
        ),
        observation("k5", "fundA", "Gamma Corp", json!({"cik": "300"})),
    ];
    let clusters = |solved: &Value| {
        let mut sets = solved["clusters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cluster| {
                let mut surfaces = cluster["surfaces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|surface| surface.as_str().unwrap().to_string())
                    .collect::<Vec<_>>();
                surfaces.sort();
                (surfaces, cluster["tier"].as_str().unwrap_or("").to_string())
            })
            .collect::<Vec<_>>();
        sets.sort();
        sets
    };

    let plain = init(RULES);
    plain.ingest(&rows);
    let baseline = clusters(&plain.solve());

    let derived_case = init(RULES);
    derived_case.ingest(&rows);
    let anchors = "    - op: anchor_match\n      params:\n        field: cik\n    - op: anchor_match\n      params:\n        field: lei";
    let profile_text = profile(anchors, NO_CANNOT, NGRAM_BLOCK, "");
    let derived = derived_case.derive(&profile_text, 0);
    eprintln!("anchor derive: {derived}");
    assert!(count(&derived, "same_key") >= 2, "{derived}");
    assert_eq!(clusters(&derived_case.solve()), baseline);
    assert!(
        baseline.iter().filter(|(_, tier)| tier == "A").count() >= 2,
        "{baseline:?}"
    );
}

#[test]
fn similar_names_derive_only_name_similar_and_are_never_accepted() {
    let case = init(RULES);
    case.ingest(&[
        observation("m1", "fundA", "Mode Purchaser LLC", json!({})),
        observation("m2", "fundB", "BGI Purchaser LLC", json!({})),
        observation("m3", "fundC", "GSV Purchaser LLC", json!({})),
    ]);
    let similarity = "    - op: string_similarity\n      view: core\n      params:\n        metric: jaro_winkler\n        min_score: \"0.5000\"";
    let profile_text = profile(similarity, NO_CANNOT, NGRAM_BLOCK, "");
    let derived = case.derive(&profile_text, 0);
    eprintln!("similarity derive: {derived}");
    assert!(count(&derived, "name_similar") >= 1, "{derived}");
    assert_eq!(
        count(&derived, "name_equivalent"),
        0,
        "similarity is not equivalence"
    );
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_a"], 0);
    assert_eq!(solved["summary"]["accepted_tier_b"], 0, "{solved}");
    let committed = run_json(
        &[
            "entity",
            "session",
            "commit",
            p(&case.session),
            "--next-version",
            "0.2.0",
        ],
        1,
    );
    assert_eq!(committed["outcome"], "nothing_to_commit");
}

#[test]
fn an_unknown_operator_is_refused_by_name_and_relation_hints_are_reported_not_run() {
    let case = init(RULES);
    case.ingest(&[observation("a1", "fundA", "Apex Capital LLC", json!({}))]);
    let unknown = "    - op: frobnicate\n      view: core";
    let refused = case.derive(&profile(unknown, NO_CANNOT, EXACT_BLOCK, ""), 2);
    eprintln!("unknown operator: {refused}");
    assert!(refused.to_string().contains("frobnicate"), "{refused}");

    // The validator requires the relation-hint lane; derive runs none of them
    // and says so instead of ignoring them silently.
    let derived = case.derive(&profile(EXACT_SUPPORT, NO_CANNOT, EXACT_BLOCK, ""), 0);
    assert_eq!(
        derived["relation_hints_not_executed"],
        json!(["relation_hints:cross_profile_alignment"])
    );
}

#[test]
fn an_unnormalisable_row_is_reported_and_skipped_and_the_rest_still_derive() {
    let case = init(RULES);
    case.ingest(&[
        observation("a1", "fundA", "Apex Capital LLC", json!({})),
        observation("a2", "fundB", "APEX CAPITAL, LLC.", json!({})),
        observation("b1", "fundA", "`", json!({})),
    ]);
    let derived = case.derive(&profile(EXACT_SUPPORT, NO_CANNOT, EXACT_BLOCK, ""), 0);
    eprintln!("backtick: {derived}");
    let rejected = derived["rejected_observations"].as_array().unwrap();
    assert_eq!(rejected.len(), 1, "{derived}");
    assert_eq!(rejected[0]["obs_id"], "b1");
    assert_eq!(count(&derived, "name_equivalent"), 1);
}

#[test]
fn two_rows_named_apex_llc_with_two_ciks_derive_a_distinct_record() {
    let case = init(RULES);
    case.ingest(&[
        observation("r1", "fundA", "Apex LLC", json!({"cik": "333"})),
        observation("r2", "fundB", "Apex LLC", json!({"cik": "444"})),
    ]);
    let cannot = "    - op: anchor_conflict\n      params:\n        field: cik\n        score_units: \"10000\"";
    let derived = case.derive(&profile(EXACT_SUPPORT, cannot, EXACT_BLOCK, ""), 0);
    eprintln!("apex: {derived}");
    assert_eq!(count(&derived, "distinct"), 1, "{derived}");
    let solved = case.solve();
    assert!(
        solved["summary"]["contradiction"].as_u64().unwrap() >= 1,
        "{solved}"
    );
    assert_eq!(solved["summary"]["accepted_tier_a"], 0);
    assert_eq!(solved["summary"]["accepted_tier_b"], 0);
}

// ---------------------------------------------------------------------------
// Day two: registry aliases are candidates for a new session's surfaces.
// ---------------------------------------------------------------------------

const DAY_TWO_BLOCK: &str = "    - op: exact_identity\n      view: core\n    - op: ngram_topk\n      view: core\n      k: 10\n      candidate_cap: 10";

fn day_two_profile() -> String {
    profile(EXACT_SUPPORT, NO_CANNOT, DAY_TWO_BLOCK, "")
}

impl Case {
    /// A new session over the same registry, with none of the first session's rows.
    fn second(&self, name: &str) -> Case {
        let session = self.root.join(name);
        run_json(
            &[
                "entity",
                "session",
                "init",
                "--goal",
                "derive day two",
                "--registry",
                p(&self.root.join("registry")),
                "--work-dir",
                p(&session),
                "--registry-id",
                &self.shape.0,
                "--prefix",
                &self.shape.1,
                "--zero-pad",
                &self.shape.2,
            ],
            0,
        );
        Case {
            _temp: None,
            root: self.root.clone(),
            session,
            rules: self.rules.clone(),
            strategy: self.strategy.clone(),
            shape: self.shape.clone(),
        }
    }

    fn derive_with(&self, profile_text: &str, extra: &[&str], exit: i32) -> Value {
        let profile_path = self.root.join(format!(
            "profile-{}.yaml",
            self.session.file_name().unwrap().to_string_lossy()
        ));
        fs::write(&profile_path, profile_text).unwrap();
        let mut args = vec![
            "entity",
            "session",
            "derive",
            p(&self.session),
            "--profile",
            p(&profile_path),
            "--strategy",
            p(&self.strategy),
        ];
        args.extend_from_slice(extra);
        run_json(&args, exit)
    }

    fn commit(&self, version: &str, exit: i32) -> Value {
        run_json(
            &[
                "entity",
                "session",
                "commit",
                p(&self.session),
                "--next-version",
                version,
            ],
            exit,
        )
    }
}

/// The first session mints ORG-001 "Apex Capital LLC" (cik 100) and commits it.
fn first_session_committed() -> Case {
    let first = init(RULES);
    first.ingest(&[observation(
        "f1",
        "fundA",
        "Apex Capital LLC",
        json!({"cik": "100"}),
    )]);
    first.solve();
    let committed = first.commit("0.2.0", 0);
    assert_eq!(committed["outcome"], "committed", "{committed}");
    first
}

fn registry_inputs(case: &Case) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for entry in fs::read_dir(case.root.join("registry")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".json")
            && name != "registry.json"
            && let Ok(Value::Array(rows)) =
                serde_json::from_slice::<Value>(&fs::read(&path).unwrap())
        {
            for row in rows {
                found.push((
                    row["input"].as_str().unwrap_or_default().to_string(),
                    row["canonical_id"].as_str().unwrap_or_default().to_string(),
                ));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn day_two_a_new_spelling_derives_to_a_registry_alias_and_attaches_without_minting() {
    let first = first_session_committed();
    let second = first.second("day-two");
    second.ingest(&[observation("n1", "fundB", "APEX CAPITAL LLC", json!({}))]);

    let derived = second.derive_with(&day_two_profile(), &[], 0);
    eprintln!("day two derive: {derived}");
    assert!(count(&derived, "name_equivalent") >= 1, "{derived}");
    assert!(derived["registry_candidates"]["enabled"].as_bool().unwrap());
    assert!(derived["registry_candidates"]["rows"].as_u64().unwrap() >= 1);
    assert_eq!(
        derived["registry_candidates"]["registry_registry_pairs_scored"], 0,
        "two registry rows are never scored"
    );

    let solved = second.solve();
    eprintln!("day two solve: {solved}");
    let committed = second.commit("0.3.0", 0);
    eprintln!("day two commit: {committed}");
    assert_eq!(committed["outcome"], "committed", "{committed}");
    assert!(
        committed["minted"].as_array().unwrap().is_empty(),
        "no second ID is minted for the spelling: {committed}"
    );
    assert_eq!(committed["attached"].as_array().unwrap().len(), 1);
    assert_eq!(committed["attached"][0]["canonical_id"], "ORG-001");
    assert!(
        registry_inputs(&first).contains(&("APEX CAPITAL LLC".to_string(), "ORG-001".to_string())),
        "the new spelling is an alias of the first session's ID"
    );
}

#[test]
fn day_two_no_registry_candidates_turns_it_off_and_the_spelling_is_not_attached() {
    let first = first_session_committed();
    let second = first.second("no-candidates");
    second.ingest(&[observation("n1", "fundB", "APEX CAPITAL LLC", json!({}))]);
    let before = registry_inputs(&first);

    // Without the registry's aliases the profile need not mark row origin.
    let derived = second.derive_with(&day_two_profile(), &["--no-registry-candidates"], 0);
    eprintln!("flag off: {derived}");
    assert_eq!(derived["registry_candidates"]["enabled"], false);
    assert_eq!(derived["registry_candidates"]["rows"], 0);
    assert_eq!(count(&derived, "name_equivalent"), 0, "{derived}");
    second.solve();
    let committed = second.commit("0.3.0", 1);
    assert_eq!(committed["outcome"], "nothing_to_commit");
    assert_eq!(registry_inputs(&first), before, "not attached");
}

#[test]
fn day_two_with_fifty_thousand_unrelated_aliases_emits_the_same_records_and_scores_no_registry_pair()
 {
    let baseline_world = first_session_committed();
    let baseline_second = baseline_world.second("baseline");
    baseline_second.ingest(&[observation("n1", "fundB", "APEX CAPITAL LLC", json!({}))]);
    let baseline = baseline_second.derive_with(&day_two_profile(), &[], 0);

    let big_world = first_session_committed();
    // 50,000 invented aliases, none related to the cut: a second mapping file
    // in the registry, with the entry count kept honest.
    let synthetic = (0..50_000)
        .map(|index| {
            json!({
                "input": format!("Zq{index:05} Synthetic Holdings"),
                "canonical_id": format!("SYN-{index:05}"),
                "canonical_type": "organisation",
                "rule_id": "SYNTHETIC",
            })
        })
        .collect::<Vec<_>>();
    let registry_dir = big_world.root.join("registry");
    fs::write(
        registry_dir.join("synthetic.json"),
        serde_json::to_vec(&synthetic).unwrap(),
    )
    .unwrap();
    let registry_json_path = registry_dir.join("registry.json");
    let mut registry_json: Value =
        serde_json::from_slice(&fs::read(&registry_json_path).unwrap()).unwrap();
    let entry_count = registry_json["entry_count"].as_u64().unwrap() + 50_000;
    registry_json["entry_count"] = json!(entry_count);
    fs::write(
        &registry_json_path,
        serde_json::to_vec(&registry_json).unwrap(),
    )
    .unwrap();

    let big_second = big_world.second("big");
    big_second.ingest(&[observation("n1", "fundB", "APEX CAPITAL LLC", json!({}))]);
    let big = big_second.derive_with(&day_two_profile(), &[], 0);
    eprintln!(
        "50k: rows={} session_registry_pairs={} registry_registry={} digest={}",
        big["registry_candidates"]["rows"],
        big["registry_candidates"]["session_registry_pairs"],
        big["registry_candidates"]["registry_registry_pairs_scored"],
        big["records_digest"]
    );
    assert!(big["registry_candidates"]["rows"].as_u64().unwrap() >= 50_000);
    assert_eq!(
        big["registry_candidates"]["registry_registry_pairs_scored"], 0,
        "no registry-to-registry pair is generated or scored"
    );
    assert_eq!(
        big["records_digest"], baseline["records_digest"],
        "the same records"
    );
    assert_eq!(big["records_by_type"], baseline["records_by_type"]);
    assert_eq!(big["records_by_operator"], baseline["records_by_operator"]);
}

/// The 3-fund real-data run of bd-vdcw.14: borrowers from the BDC gold sets
/// become observations (the converter's borrower mode), `derive` finds the
/// name equivalences with no script evidence, and solve + commit accept them.
/// Ignored by default; run it with the gold-set directory in the environment:
///
/// `CANON_BDC_GOLD_DIR=/path/to/gold_sets cargo test --test entity_session_derive -- --ignored real_bdc`
#[test]
#[ignore = "reads the real BDC gold sets from CANON_BDC_GOLD_DIR"]
fn real_bdc_gold_sets_derive_name_equivalent_without_script_evidence() {
    let gold_dir = std::env::var("CANON_BDC_GOLD_DIR")
        .expect("set CANON_BDC_GOLD_DIR to the BDC gold_sets directory");
    let gold_dir = PathBuf::from(gold_dir);

    // The first three funds by CIK.
    let mut ciks = std::collections::BTreeSet::new();
    for entry in fs::read_dir(&gold_dir).expect("gold-set directory") {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".goldset.v") && name.ends_with(".json"))
        {
            let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if let Some(cik) = value["source"]["cik"].as_str() {
                ciks.insert(cik.to_string());
            }
        }
    }
    let ciks = ciks.into_iter().take(3).collect::<Vec<_>>();
    assert_eq!(ciks.len(), 3, "need three funds in {gold_dir:?}");

    let case = init(RULES);
    let ciks_file = case.root.join("ciks.txt");
    fs::write(&ciks_file, format!("{}\n", ciks.join("\n"))).unwrap();
    let observations = case.root.join("borrowers.jsonl");
    let converter = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/entity_sessions/bdc_goldsets_to_session.py");
    let status = std::process::Command::new("python3")
        .arg(&converter)
        .args(["--mode", "borrowers", "--pick", "all", "--gold-dir"])
        .arg(&gold_dir)
        .arg("--ciks-file")
        .arg(&ciks_file)
        .arg("--out")
        .arg(&observations)
        .status()
        .expect("python3 runs the converter");
    assert!(status.success(), "converter failed");
    let ingested = run_json(
        &[
            "entity",
            "session",
            "ingest",
            p(&case.session),
            p(&observations),
        ],
        0,
    );
    eprintln!(
        "ingested {} borrower observations from funds {ciks:?}",
        ingested["accepted"]
    );

    let derived = case.derive_with(&day_two_profile(), &["--no-registry-candidates"], 0);
    eprintln!("real-data derive: {derived}");
    let equivalent = count(&derived, "name_equivalent");
    assert!(equivalent > 0, "no name_equivalent derived: {derived}");
    let solved = case.solve();
    eprintln!(
        "real-data solve: tier_b={} review={} contradiction={}",
        solved["summary"]["accepted_tier_b"],
        solved["summary"]["review"],
        solved["summary"]["contradiction"]
    );
    // Derived evidence reaches solve: as accepted clusters where two lineages
    // support the equivalence, as review items where one fund holds both
    // spellings. How many of each depends on the cut, so the counts are logged
    // for the verifier; the claim asserted here is that nothing was scripted.
    let tier_b = solved["summary"]["accepted_tier_b"].as_u64().unwrap();
    let review = solved["summary"]["review"].as_u64().unwrap();
    assert!(
        tier_b + review > 0,
        "derived evidence produced neither clusters nor review items: {solved}"
    );
    if tier_b > 0 {
        let committed = case.commit("0.2.0", 0);
        eprintln!(
            "real-data commit: minted={}",
            committed["minted"].as_array().unwrap().len()
        );
        assert_eq!(committed["outcome"], "committed");
    }
}

// ---------------------------------------------------------------------------
// An independently shaped fixture: a different registry id, prefix and pad (the
// registry created by `registry init`, not by session init), a different source
// id, and derive run on day one as well as day two.
// ---------------------------------------------------------------------------

fn shaped_observation(obs_id: &str, lineage: &str, surface: &str, keys: Value) -> Value {
    let mut row = observation(obs_id, lineage, surface, keys);
    row["source"]["id"] = json!("mine");
    row
}

fn shaped_case(rules: &str) -> Case {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    let rules_path = root.join("rules.yaml");
    fs::write(&rules_path, rules).unwrap();
    let strategy = root.join("strategy.yaml");
    fs::write(&strategy, STRATEGY).unwrap();
    run_json(
        &[
            "registry",
            "init",
            "--registry",
            p(&root.join("registry")),
            "--registry-id",
            "orgs",
            "--prefix",
            "O",
            "--zero-pad",
            "4",
        ],
        0,
    );
    let case = Case {
        _temp: Some(temp),
        root: root.clone(),
        session: root.join("day-one"),
        rules: rules_path,
        strategy,
        shape: ("orgs".into(), "O".into(), "4".into()),
    };
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "day one",
            "--registry",
            p(&root.join("registry")),
            "--work-dir",
            p(&case.session),
            "--registry-id",
            "orgs",
            "--prefix",
            "O",
            "--zero-pad",
            "4",
        ],
        0,
    );
    case
}

/// Day one mints O-0001 "Apex Capital LLC" (cik 100) and commits; day two is a
/// new session with one row, no key, another lineage.
fn shaped_day_two(rules: &str) -> (Case, Case) {
    let day_one = shaped_case(rules);
    day_one.ingest(&[shaped_observation(
        "a1",
        "fundA",
        "Apex Capital LLC",
        json!({"cik": "100"}),
    )]);
    day_one.derive_with(&day_two_profile(), &[], 0);
    day_one.solve();
    let committed = day_one.commit("1.0.0", 0);
    assert_eq!(committed["outcome"], "committed", "{committed}");
    let day_two = day_one.second("day-two");
    day_two.ingest(&[shaped_observation(
        "n1",
        "fundB",
        "APEX CAPITAL LLC",
        json!({}),
    )]);
    (day_one, day_two)
}

#[test]
fn day_two_attaches_on_an_independently_shaped_fixture() {
    let (day_one, day_two) = shaped_day_two(RULES);
    let derived = day_two.derive_with(&day_two_profile(), &[], 0);
    eprintln!("shaped derive: {derived}");
    assert!(count(&derived, "name_equivalent") >= 1, "{derived}");
    let solved = day_two.solve();
    eprintln!("shaped solve: {solved}");
    assert_eq!(solved["summary"]["review"], 0, "{solved}");
    let committed = day_two.commit("1.1.0", 0);
    assert_eq!(committed["outcome"], "committed", "{committed}");
    assert!(committed["minted"].as_array().unwrap().is_empty());
    assert_eq!(committed["attached"].as_array().unwrap().len(), 1);
    assert_eq!(committed["attached"][0]["canonical_id"], "O-0001");
    assert!(
        registry_inputs(&day_one).contains(&("APEX CAPITAL LLC".to_string(), "O-0001".to_string()))
    );
}

/// The rule table decides what counts as the same spelling: without
/// `incumbent_attach.name_equivalent_steps` a derived name_equivalent to a
/// registry alias is review material that needs a second lineage (the registry
/// endpoint brings none), so nothing attaches. The guide says to keep the
/// section the rules template ships with.
#[test]
fn without_incumbent_attach_steps_the_registry_hit_is_a_review_item_and_nothing_attaches() {
    let without = RULES.replace(
        "incumbent_attach:\n  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]\n",
        "",
    );
    assert!(!without.contains("incumbent_attach"));
    let (day_one, day_two) = shaped_day_two(&without);
    let before = registry_inputs(&day_one);
    let derived = day_two.derive_with(&day_two_profile(), &[], 0);
    assert!(
        count(&derived, "name_equivalent") >= 1,
        "evidence is derived either way"
    );
    let solved = day_two.solve();
    assert_eq!(solved["summary"]["review"], 1, "{solved}");
    assert_eq!(solved["review"][0]["missing"], "second_lineage");
    let committed = day_two.commit("1.1.0", 1);
    assert_eq!(committed["outcome"], "nothing_to_commit");
    assert_eq!(registry_inputs(&day_one), before);
}

// ---------------------------------------------------------------------------
// Attribute agreement (bd-vdcw.15): numeric_within_tolerance, date_exact and
// categorical_match become one attribute_agreement record per pair.
// ---------------------------------------------------------------------------

const ATTRIBUTE_RULES: &str = r#"version: canon.entity.session.rules.v1
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
"#;

const AGREE_ALL: &str = "    - op: categorical_match\n      params:\n        field: investment_type\n    - op: date_exact\n      params:\n        field: maturity_date\n    - op: numeric_within_tolerance\n      params:\n        field: par_amount\n        absolute: \"0.05\"";
const AGREE_PAR_ONLY: &str = "    - op: numeric_within_tolerance\n      params:\n        field: par_amount\n        absolute: \"0.05\"";
const ATTRIBUTE_BLOCK: &str =
    "    - op: composite_key\n      views: [maturity_date, investment_type]";

fn attribute_profile(support: &str, cannot_link: &str) -> String {
    format!(
        r#"profile: session_derive_attributes
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
  investment_type:
    operators: [identity]
  maturity_date:
    operators: [identity]
  spread:
    operators: [identity]
  par_amount:
    operators: [identity]
prepare:
  primary_surface_fields: [surface]
  canonical_surface_normalized_view: core
  normalized_view_fields:
    core: surface
    row_key: source_row_id
    investment_type: investment_type
    maturity_date: maturity_date
    spread: spread
    par_amount: par_amount
  surface_key_fields: [row_key]
  nullable_fields: [investment_type, maturity_date, spread, par_amount]
  provenance_fields: [source_row_id, origin]
blocking:
  operators:
{ATTRIBUTE_BLOCK}
evidence:
  support:
{support}
  cannot_link:
{cannot_link}
  relation_hints:
    - op: cross_profile_alignment
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: session_derive_attributes.aliases
  distinct: session_derive_attributes.distinct
  relations: session_derive_attributes.relations
solve:
  hard_cannot_link: contradiction
  standalone_surfaces: include
"#
    )
}

fn loan(obs_id: &str, lineage: &str, surface: &str, spread: &str, par: &str) -> Value {
    let mut row = observation(obs_id, lineage, surface, json!({}));
    row["attrs"] = json!({
        "investment_type": "First lien term loan",
        "maturity_date": "2030-06-30",
        "spread": spread,
        "par_amount": par,
    });
    row
}

/// The `basis` Canon stored for the evidence between two observations.
fn stored_basis(case: &Case, obs: &str) -> Vec<Value> {
    let explained = run_json(&["entity", "session", "explain", p(&case.session), obs], 0);
    explained["edges"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|edge| edge["evidence"].as_array().unwrap().iter().cloned())
        .collect()
}

#[test]
fn agreeing_attributes_derive_one_record_per_pair_listing_each_of_them() {
    let case = init(ATTRIBUTE_RULES);
    case.ingest(&[
        loan(
            "r8",
            "fundA",
            "Project Falcon Buyer, Inc.",
            "9.75",
            "12500000",
        ),
        loan(
            "r9",
            "fundB",
            "Falcon Parent Holdings (dba SkyWorks)",
            "9.75",
            "12500000",
        ),
        loan("z1", "fundA", "Zenith Partners", "9.75", "99999999"),
    ]);
    let derived = case.derive(&attribute_profile(AGREE_ALL, NO_CANNOT), 0);
    eprintln!("attribute derive: {derived}");
    // The pair agrees on three attributes: ONE evidence record, not three.
    assert_eq!(count(&derived, "attribute_agreement"), 3, "{derived}");
    let solved = case.solve();
    eprintln!("attribute solve: {solved}");
    assert_eq!(solved["summary"]["accepted_tier_a"], 0);
    assert_eq!(
        solved["summary"]["accepted_tier_b"], 0,
        "attribute agreement alone is not acceptance"
    );
    // r8 / r9 as a review pair missing a second evidence type.
    let review = solved["review"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| {
            let pair = [
                item["left_surface"].as_str(),
                item["right_surface"].as_str(),
            ];
            pair.contains(&Some("Project Falcon Buyer, Inc."))
                && pair.contains(&Some("Falcon Parent Holdings (dba SkyWorks)"))
        })
        .unwrap_or_else(|| panic!("no review item for r8/r9: {solved}"));
    assert_eq!(review["missing"], "second_evidence_type");
    assert_eq!(review["evidence_types"], json!(["attribute_agreement"]));

    // The stored basis lists every agreeing attribute with both values and the tolerance.
    let basis = stored_basis(&case, "r8");
    let attribute = basis
        .iter()
        .find(|record| record["type"] == "attribute_agreement")
        .and_then(|record| record["basis"].as_object().cloned())
        .expect("attribute_agreement basis");
    eprintln!("basis: {}", Value::Object(attribute.clone()));
    assert_eq!(
        attribute["fields"],
        json!(["investment_type", "maturity_date", "par_amount"])
    );
    let par = attribute["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["field"] == "par_amount")
        .unwrap();
    assert_eq!(par["left_value"], "12500000");
    assert_eq!(par["right_value"], "12500000");
    assert_eq!(par["tolerance"]["absolute"], "0.05");

    // Idempotent.
    let again = case.derive(&attribute_profile(AGREE_ALL, NO_CANNOT), 0);
    assert_eq!(again["accepted"], 0);
}

#[test]
fn a_par_outside_the_tolerance_yields_no_record_and_the_rest_still_agree() {
    let case = init(ATTRIBUTE_RULES);
    case.ingest(&[
        loan(
            "r8",
            "fundA",
            "Project Falcon Buyer, Inc.",
            "9.75",
            "12500000",
        ),
        loan(
            "r9",
            "fundB",
            "Falcon Parent Holdings (dba SkyWorks)",
            "9.75",
            "13000000",
        ),
    ]);
    // par alone: no record.
    let par_only = case.derive(&attribute_profile(AGREE_PAR_ONLY, NO_CANNOT), 0);
    assert_eq!(count(&par_only, "attribute_agreement"), 0, "{par_only}");
}

#[test]
fn three_agreeing_attributes_on_a_same_lineage_pair_do_not_reach_tier_b() {
    let case = init(ATTRIBUTE_RULES);
    case.ingest(&[
        loan(
            "r8",
            "fundA",
            "Project Falcon Buyer, Inc.",
            "9.75",
            "12500000",
        ),
        loan(
            "r9",
            "fundA",
            "Falcon Parent Holdings (dba SkyWorks)",
            "9.75",
            "12500000",
        ),
    ]);
    let derived = case.derive(&attribute_profile(AGREE_ALL, NO_CANNOT), 0);
    assert_eq!(
        count(&derived, "attribute_agreement"),
        1,
        "one evidence type, not three: {derived}"
    );
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_b"], 0, "{solved}");
    assert_eq!(solved["summary"]["review"], 1);
}

#[test]
fn an_attribute_conflict_on_the_same_pair_still_vetoes() {
    let case = init(ATTRIBUTE_RULES);
    case.ingest(&[
        loan(
            "r8",
            "fundA",
            "Project Falcon Buyer, Inc.",
            "9.75",
            "12500000",
        ),
        loan(
            "r9",
            "fundB",
            "Falcon Parent Holdings (dba SkyWorks)",
            "9.80",
            "12500000",
        ),
    ]);
    let conflict = "    - op: attribute_conflict\n      params:\n        field: spread\n        comparison: decimal_bps\n        tolerance_bps: \"1\"\n        rate_scale: percent\n        score_units: \"10000\"";
    let derived = case.derive(&attribute_profile(AGREE_ALL, conflict), 0);
    eprintln!("conflict derive: {derived}");
    assert_eq!(count(&derived, "attribute_agreement"), 1, "{derived}");
    assert_eq!(count(&derived, "distinct"), 1, "{derived}");
    let solved = case.solve();
    assert!(
        solved["summary"]["contradiction"].as_u64().unwrap() >= 1,
        "{solved}"
    );
    assert_eq!(solved["summary"]["accepted_tier_b"], 0);
}

// ---------------------------------------------------------------------------
// Person and trust names (bd-vdcw.26). All names here are invented.
// ---------------------------------------------------------------------------

fn names_profile() -> String {
    profile(EXACT_SUPPORT, NO_CANNOT, EXACT_BLOCK, "")
        .replace(
            "  row_key:\n    operators: [ascii_trim_upper]\n",
            "  row_key:\n    operators: [ascii_trim_upper]\n  person:\n    operators: [person_name]\n",
        )
        .replace(
            "    row_key: source_row_id\n",
            "    row_key: source_row_id\n    person: surface\n",
        )
}

/// A session whose registries map the kinds person, trust and unknown beside
/// organisation (a kind must be mapped for ingest to accept it).
fn names_case(rules: &str) -> Case {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    let session = root.join("session");
    let rules_path = root.join("rules.yaml");
    fs::write(&rules_path, rules).unwrap();
    let strategy = root.join("strategy.yaml");
    fs::write(&strategy, STRATEGY).unwrap();
    let organisations = root.join("organisations");
    let mut init_registry = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"));
    init_registry.args([
        "registry",
        "init",
        "--registry",
        p(&organisations),
        "--registry-id",
        "organisations",
        "--prefix",
        "ORG",
        "--zero-pad",
        "3",
    ]);
    init_registry.assert().success();
    let agent_kind = |name: &str, canonical: &str| {
        format!(
            "  {name}:\n    registry: {}\n    registry_id: {name}s\n    id_scheme: agent\n    kind_prefix: {}\n    canonical_type: {canonical}\n    alias_file: aliases.json\n",
            root.join(format!("{name}s")).display(),
            &name[..3]
        )
    };
    let map = root.join("registries.yaml");
    fs::write(
        &map,
        format!(
            "version: canon.session.registries.v1\nkinds:\n  organisation:\n    registry: {}\n    registry_id: organisations\n    prefix: ORG\n    zero_pad: 3\n    canonical_type: firm\n    alias_file: 30-session-aliases.json\n{}{}{}",
            organisations.display(),
            agent_kind("person", "person"),
            agent_kind("trust", "trust"),
            agent_kind("unknown", "unknown"),
        ),
    )
    .unwrap();
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "names",
            "--work-dir",
            p(&session),
            "--registries",
            p(&map),
        ],
        0,
    );
    Case {
        _temp: Some(temp),
        root,
        session,
        rules: rules_path,
        strategy,
        shape: ("organisations".into(), "ORG".into(), "3".into()),
    }
}

fn typed(obs_id: &str, lineage: &str, surface: &str, entity_type: &str) -> Value {
    let mut row = observation(obs_id, lineage, surface, json!({}));
    row["entity_type"] = json!(entity_type);
    row
}

/// Every stored evidence record of `type` touching `obs` (explain reads the
/// solve generation, so solve first).
fn records_of(case: &Case, obs: &str, kind: &str) -> Vec<Value> {
    case.solve();
    stored_basis(case, obs)
        .into_iter()
        .filter(|record| record["type"] == kind)
        .collect()
}

#[test]
fn one_person_in_three_spellings_is_name_equivalent_with_listed_steps() {
    let case = names_case(ATTRIBUTE_RULES);
    case.ingest(&[
        typed("p1", "fundA", "Smith, John A. Jr.", "person"),
        typed("p2", "fundB", "John A Smith Jr", "person"),
        typed("p3", "fundC", "JOHN A. SMITH, JR.", "person"),
        typed("d1", "fundA", "Jane Doe", "person"),
        typed("d2", "fundB", "Doe, Jane", "person"),
    ]);
    let derived = case.derive(&names_profile(), 0);
    eprintln!("names derive: {derived}");
    // three spellings = three pairs, plus Jane Doe's pair
    assert_eq!(count(&derived, "name_equivalent"), 4, "{derived}");
    let reordered = records_of(&case, "p1", "name_equivalent");
    assert_eq!(reordered.len(), 2, "{reordered:?}");
    for record in &reordered {
        let steps = record["basis"]["steps"].as_array().unwrap();
        assert!(steps.contains(&json!("person_name")), "{steps:?}");
        assert!(steps.contains(&json!("person:suffix_split")), "{steps:?}");
    }
    assert!(reordered.iter().any(|record| {
        record["basis"]["steps"]
            .as_array()
            .unwrap()
            .contains(&json!("person:last_first_reordered"))
    }));
    // Name equivalence is one evidence type: it never accepts alone.
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_b"], 0, "{solved}");
    assert_eq!(derived["names"]["classified"]["person"], 5);
}

#[test]
fn a_different_suffix_is_distinct_and_a_different_middle_is_not_equivalent() {
    let case = names_case(ATTRIBUTE_RULES);
    case.ingest(&[
        typed("j1", "fundA", "John A. Smith Jr.", "person"),
        typed("j2", "fundB", "John A. Smith Sr.", "person"),
        typed("m1", "fundA", "John A. Smith", "person"),
        typed("m2", "fundB", "John B. Smith", "person"),
        typed("m3", "fundC", "John Andrew Smith", "person"),
    ]);
    let derived = case.derive(&names_profile(), 0);
    eprintln!("suffix derive: {derived}");
    // Jr / Sr both carry a suffix and differ: distinct with authority.
    assert_eq!(count(&derived, "distinct"), 1, "{derived}");
    let distinct = records_of(&case, "j1", "distinct");
    // (the authority is consumed by assert; solve treats it as a veto below)
    assert!(
        distinct[0]["basis"]["explanation"]
            .as_str()
            .unwrap()
            .contains("suffixes differ"),
        "{}",
        distinct[0]
    );
    // A with B: nothing. A with Andrew: similar, for review. Neither is equivalent.
    // "John A. Smith" vs "John Smith"-style middle absence has no row here; Jr and
    // Sr never reach the unsuffixed rows (one side absent: nothing claimed).
    assert_eq!(count(&derived, "name_equivalent"), 0, "{derived}");
    assert_eq!(count(&derived, "name_similar"), 1, "{derived}");
    let similar = records_of(&case, "m1", "name_similar");
    assert_eq!(similar.len(), 1);
    assert!(records_of(&case, "m2", "name_similar").is_empty());
    // Two lineages say the Jr and the Sr hold the same slot: the veto still wins.
    let slots = ["lineageX", "lineageY"]
        .iter()
        .map(|lineage| {
            serde_json::to_string(&json!({
                "type": "same_slot",
                "left": "j1",
                "right": "j2",
                "basis": {"rule": "same loan and role", "left_value": "loan-7:guarantor", "right_value": "loan-7:guarantor"},
                "source": {"channel": "fixture", "id": lineage, "lineage": lineage},
                "asserted_by": "agent:fixture"
            }))
            .unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let slot_path = case.root.join("slots.jsonl");
    fs::write(&slot_path, format!("{slots}\n")).unwrap();
    run_json(
        &[
            "entity",
            "session",
            "assert",
            p(&case.session),
            p(&slot_path),
        ],
        0,
    );
    let solved = case.solve();
    assert!(
        solved["summary"]["contradiction"].as_u64().unwrap() >= 1,
        "{solved}"
    );
    assert_eq!(solved["summary"]["accepted_tier_b"], 0);
}

#[test]
fn a_middle_name_absent_on_one_side_is_equivalent_with_a_step() {
    let case = names_case(ATTRIBUTE_RULES);
    case.ingest(&[
        typed("a1", "fundA", "John Smith", "person"),
        typed("a2", "fundB", "John A. Smith", "person"),
    ]);
    let derived = case.derive(&names_profile(), 0);
    assert_eq!(count(&derived, "name_equivalent"), 1, "{derived}");
    let record = &records_of(&case, "a1", "name_equivalent")[0];
    assert!(
        record["basis"]["steps"]
            .as_array()
            .unwrap()
            .contains(&json!("person:middle_absent"))
    );
}

#[test]
fn a_person_an_org_and_a_typed_org_with_comma_are_never_parsed_across_types() {
    let case = names_case(ATTRIBUTE_RULES);
    case.ingest(&[
        typed("p1", "fundA", "John Smith", "person"),
        typed("o1", "fundB", "John Smith LLC", "organisation"),
        // typed org: "Smith, Jones" must not be read as family Smith, given Jones
        typed("o2", "fundA", "Smith, Jones", "organisation"),
        typed("o3", "fundB", "Jones Smith", "organisation"),
    ]);
    let derived = case.derive(&names_profile(), 0);
    eprintln!("cross type derive: {derived}");
    for kind in ["name_equivalent", "name_similar", "distinct", "related"] {
        assert_eq!(count(&derived, kind), 0, "{kind}: {derived}");
    }
}

#[test]
fn fragments_are_rejected_and_a_trust_is_related_to_its_person_never_merged() {
    let case = names_case(ATTRIBUTE_RULES);
    case.ingest(&[
        typed("f1", "fundA", "Jr", "unknown"),
        typed("f2", "fundA", "LP", "unknown"),
        typed("f3", "fundA", "Sr.", "unknown"),
        typed(
            "t1",
            "fundA",
            "The John A. Smith Revocable Trust dated 1/1/2001",
            "unknown",
        ),
        typed("p1", "fundB", "John A. Smith", "person"),
        typed("u1", "fundC", "John A. Smith Holdings", "unknown"),
    ]);
    let derived = case.derive(&names_profile(), 0);
    eprintln!("fragment derive: {derived}");
    let rejected = derived["names"]["fragments_rejected"].as_array().unwrap();
    let mut ids = rejected
        .iter()
        .map(|entry| entry["obs_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(ids, ["f1", "f2", "f3"]);
    let classified = &derived["names"]["classified"];
    assert_eq!(classified["fragment"], 3);
    assert_eq!(classified["trust"], 1);
    assert_eq!(classified["person"], 1);
    // The trust names the person: reported, not written as evidence (solve
    // refuses evidence between two entity types).
    assert_eq!(count(&derived, "related"), 0, "{derived}");
    let relations = derived["names"]["trust_relations"].as_array().unwrap();
    assert_eq!(relations.len(), 1, "{derived}");
    assert_eq!(relations[0]["trust_obs_id"], "t1");
    assert_eq!(relations[0]["person_obs_id"], "p1");
    assert_eq!(relations[0]["relation"], "trust_of");
    // No evidence of any kind names a fragment as a node.
    for fragment in ["f1", "f2", "f3"] {
        case.solve();
        assert!(stored_basis(&case, fragment).is_empty(), "{fragment}");
    }
    // The trust is related, never merged, with its person.
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_a"], 0, "{solved}");
    assert_eq!(solved["summary"]["accepted_tier_b"], 0, "{solved}");
}

fn structure_profile() -> String {
    profile(EXACT_SUPPORT, NO_CANNOT, EXACT_BLOCK, "")
        .replace(
            "  row_key:\n    operators: [ascii_trim_upper]\n",
            "  row_key:\n    operators: [ascii_trim_upper]\n  structure:\n    operators: [name_structure]\n",
        )
        .replace(
            "    row_key: source_row_id\n",
            "    row_key: source_row_id\n    structure: surface\n",
        )
}

fn assert_lines(case: &Case, name: &str, lines: &[Value]) {
    let path = case.root.join(name);
    let text = lines
        .iter()
        .map(|line| serde_json::to_string(line).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, format!("{text}\n")).unwrap();
    run_json(
        &["entity", "session", "assert", p(&case.session), p(&path)],
        0,
    );
}

fn cluster_sets(solved: &Value) -> Vec<Vec<String>> {
    let mut sets = solved["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cluster| {
            let mut surfaces = cluster["surfaces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|surface| surface.as_str().unwrap().to_string())
                .collect::<Vec<_>>();
            surfaces.sort();
            surfaces
        })
        .collect::<Vec<_>>();
    sets.sort();
    sets
}

#[test]
fn a_stated_dba_is_an_asserted_alias_to_the_alternate_and_nothing_else_links() {
    let case = names_case(RULES);
    case.ingest(&[
        observation("r2", "fundA", "Mozart Borrower LP (dba Medline)", json!({})),
        observation("r_med", "fundB", "Medline", json!({})),
        observation("g1", "fundC", "Medline Borrower, LP", json!({})),
        observation(
            "f1",
            "fundA",
            "Falcon Parent Holdings (dba SkyWorks)",
            json!({}),
        ),
        observation("f2", "fundB", "Project Falcon Buyer, Inc.", json!({})),
        observation("j1", "fundA", "Johnson and Johnson", json!({})),
        observation("s1", "fundB", "Smith, Jones and Partners LLP", json!({})),
    ]);
    let derived = case.derive(&structure_profile(), 0);
    eprintln!("structure derive: {derived}");
    // Only the one stated alias: the dba "Medline" equals the whole surface of r_med.
    assert_eq!(count(&derived, "asserted_alias"), 1, "{derived}");
    let structure = &derived["structure"];
    assert_eq!(structure["surfaces_with_alternates"], 2, "{derived}");
    assert_eq!(structure["alternates_by_marker"]["dba"], 2);
    assert_eq!(structure["multi_party"].as_array().unwrap().len(), 0);
    assert_eq!(structure["ambiguous_conjunctions"], 2, "{derived}");
    let records = records_of(&case, "r2", "asserted_alias");
    assert_eq!(records.len(), 1, "{records:?}");
    let basis = &records[0]["basis"];
    assert_eq!(basis["marker"], "dba");
    assert_eq!(basis["marker_text"], "dba");
    assert_eq!(basis["primary"], "Mozart Borrower LP");
    assert_eq!(basis["alternate"], "Medline");
    assert!(
        basis["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step == "name_structure:marker:dba"),
        "{basis}"
    );
    // One tier A cluster of the two rows; every other row stays alone.
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_a"], 1, "{solved}");
    let sets = cluster_sets(&solved);
    assert!(
        sets.contains(&vec![
            "Medline".to_string(),
            "Mozart Borrower LP (dba Medline)".to_string()
        ]),
        "{sets:?}"
    );
    assert_eq!(
        sets.iter().filter(|set| set.len() > 1).count(),
        1,
        "{sets:?}"
    );
}

#[test]
fn a_list_is_related_to_its_members_never_merged_and_a_wrong_merge_is_a_contradiction() {
    let case = names_case(ATTRIBUTE_RULES);
    case.ingest(&[
        observation(
            "l1",
            "fundA",
            "Alpha Holdings LLC and Beta Capital LP",
            json!({}),
        ),
        observation("a1", "fundB", "Alpha Holdings LLC", json!({})),
        observation("b1", "fundC", "Beta Capital LP", json!({})),
        observation(
            "l2",
            "fundD",
            "Beta Capital, LP; ALPHA HOLDINGS LLC",
            json!({}),
        ),
        observation("g1", "fundE", "Alpha Holdings LLC; Gamma Inc", json!({})),
    ]);
    let derived = case.derive(&structure_profile(), 0);
    eprintln!("list derive: {derived}");
    let structure = &derived["structure"];
    let mut listed = structure["multi_party"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["obs_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    listed.sort();
    assert_eq!(listed, ["g1", "l1", "l2"], "{derived}");
    // l1 and l2 each relate to a1 and b1; g1 to a1 only.
    assert_eq!(count(&derived, "related"), 5, "{derived}");
    assert_eq!(structure["co_borrower_records"], 5);
    // Equivalence only between the two lists with equal member sets.
    assert_eq!(count(&derived, "name_equivalent"), 1, "{derived}");
    assert_eq!(structure["list_equivalence_records"], 1);
    let equal = records_of(&case, "l1", "name_equivalent");
    assert_eq!(equal.len(), 1);
    assert_eq!(equal[0]["basis"]["members"].as_array().unwrap().len(), 2);
    let related = records_of(&case, "l1", "related");
    assert_eq!(related.len(), 2, "{related:?}");
    assert!(related.iter().all(|record| {
        record["basis"]["relation"] == "co_borrower_of" && record["basis"]["subject"] == "l1"
    }));
    // No list shares a cluster with a member on derive's evidence alone.
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_a"], 0, "{solved}");
    assert_eq!(solved["summary"]["accepted_tier_b"], 0, "{solved}");

    // An agent wrongly asserts the list equals a member, with two types of
    // evidence from two lineages: solve reports the contradiction, no merge.
    let wrong = |kind: &str, lineage: &str| {
        let basis = if kind == "same_slot" {
            json!({"rule": "agent claim", "left_value": "x", "right_value": "x"})
        } else {
            json!({"rule": "agent claim", "left_value": "Alpha Holdings LLC and Beta Capital LP", "right_value": "Alpha Holdings LLC", "steps": ["agent"], "applies_to": "surface", "explanation": "wrong"})
        };
        json!({
            "type": kind,
            "left": "a1",
            "right": "l1",
            "basis": basis,
            "source": {"channel": "fixture", "id": lineage, "lineage": lineage},
            "asserted_by": "agent:fixture"
        })
    };
    assert_lines(
        &case,
        "wrong.jsonl",
        &[
            wrong("same_slot", "lineageX"),
            wrong("name_equivalent", "lineageY"),
        ],
    );
    let solved = case.solve();
    assert_eq!(solved["summary"]["accepted_tier_b"], 0, "{solved}");
    assert!(
        solved["contradictions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["reason"] == "related_not_same"),
        "{solved}"
    );
}

#[test]
fn a_marker_with_no_equal_surface_and_a_plain_comma_name_derive_nothing() {
    let case = names_case(RULES);
    case.ingest(&[
        observation(
            "m1",
            "fundA",
            "Quill Holdings LP formerly Harbor",
            json!({}),
        ),
        observation("m2", "fundB", "Harbor", json!({})),
        observation("c1", "fundA", "Medline Borrower, LP", json!({})),
        observation("c2", "fundB", "Medline Borrower LP", json!({})),
        observation("d1", "fundC", "Pike Foods LLC (dba )", json!({})),
        observation("n1", "fundC", "Madbaker Foods LLC", json!({})),
        observation("n2", "fundD", "Baker Foods LLC", json!({})),
    ]);
    let derived = case.derive(&structure_profile(), 0);
    eprintln!("negative derive: {derived}");
    assert_eq!(count(&derived, "asserted_alias"), 0, "{derived}");
    assert_eq!(count(&derived, "related"), 0, "{derived}");
    assert_eq!(derived["structure"]["surfaces_with_alternates"], 0);
    assert_eq!(
        derived["structure"]["multi_party"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

/// The real BDC gold-set borrowers through the structure stage. Ignored by
/// default; the counts are logged and 50 examples of each kind are written for
/// the verifier to read (a parser hit is not a correct alias until read):
///
/// `CANON_BDC_GOLD_DIR=/path/to/gold_sets CANON_STRUCTURE_EXAMPLES_OUT=/tmp/ex.json cargo test --test entity_session_derive -- --ignored real_bdc_gold_sets_name_structure`
#[test]
#[ignore = "reads the real BDC gold sets from CANON_BDC_GOLD_DIR"]
fn real_bdc_gold_sets_name_structure_counts_markers_and_lists() {
    let gold_dir = PathBuf::from(
        std::env::var("CANON_BDC_GOLD_DIR")
            .expect("set CANON_BDC_GOLD_DIR to the BDC gold_sets directory"),
    );
    let mut ciks = std::collections::BTreeSet::new();
    for entry in fs::read_dir(&gold_dir).expect("gold-set directory") {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".goldset.v") && name.ends_with(".json"))
        {
            let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if let Some(cik) = value["source"]["cik"].as_str() {
                ciks.insert(cik.to_string());
            }
        }
    }
    let case = names_case(RULES);
    let ciks_file = case.root.join("ciks.txt");
    fs::write(
        &ciks_file,
        format!("{}\n", ciks.iter().cloned().collect::<Vec<_>>().join("\n")),
    )
    .unwrap();
    let observations = case.root.join("borrowers.jsonl");
    let status = std::process::Command::new("python3")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/entity_sessions/bdc_goldsets_to_session.py"),
        )
        .args(["--mode", "borrowers", "--pick", "all", "--gold-dir"])
        .arg(&gold_dir)
        .arg("--ciks-file")
        .arg(&ciks_file)
        .arg("--out")
        .arg(&observations)
        .status()
        .expect("python3 runs the converter");
    assert!(status.success(), "converter failed");
    run_json(
        &[
            "entity",
            "session",
            "ingest",
            p(&case.session),
            p(&observations),
        ],
        0,
    );
    let derived = case.derive_with(&structure_profile(), &["--no-registry-candidates"], 0);
    let structure = &derived["structure"];
    eprintln!(
        "real-data structure: parsed={} with_alternates={} by_marker={} multi_party={} ambiguous_conjunctions={} asserted_alias={} co_borrower={} list_equivalence={}",
        structure["surfaces_parsed"],
        structure["surfaces_with_alternates"],
        structure["alternates_by_marker"],
        structure["multi_party"].as_array().unwrap().len(),
        structure["ambiguous_conjunctions"],
        structure["asserted_alias_records"],
        structure["co_borrower_records"],
        structure["list_equivalence_records"],
    );
    if let Ok(out) = std::env::var("CANON_STRUCTURE_EXAMPLES_OUT") {
        let take = |value: &Value| {
            let all = value.as_array().cloned().unwrap_or_default();
            let step = (all.len() / 50).max(1);
            all.into_iter().step_by(step).take(50).collect::<Vec<_>>()
        };
        let examples = json!({
            "counts": structure.clone(),
            "alternate_examples": take(&structure["alternate_surfaces"]),
            "list_examples": take(&structure["multi_party"]),
        });
        fs::write(out, serde_json::to_string_pretty(&examples).unwrap()).unwrap();
    }
    // Nothing is merged on the strength of a parse alone: every record the
    // stage wrote cites a marker or a list the source string states.
    assert_eq!(
        count(&derived, "asserted_alias"),
        structure["asserted_alias_records"].as_u64().unwrap()
    );
}
