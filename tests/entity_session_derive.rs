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
                "organisations",
                "--prefix",
                "ORG",
                "--zero-pad",
                "3",
            ],
            0,
        );
        Case {
            _temp: None,
            root: self.root.clone(),
            session,
            rules: self.rules.clone(),
            strategy: self.strategy.clone(),
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
