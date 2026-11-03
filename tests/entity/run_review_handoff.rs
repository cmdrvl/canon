#![forbid(unsafe_code)]

//! Regression coverage for the `entity run` -> `review export` -> `review
//! import` hand-off and for the profile `protected_token_conflict` operator
//! (bd-wjbf, bd-0al7). Every test drives the shipped CLI binary.

use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

const ROWS: &str = "source_row_id,deal_id,loan_id,property_id,raw_tenant_name\n\
r1,D1,L1,P1,LD Sub III LLC\n\
r2,D1,L1,P2,LD Sub IV LLC\n\
r3,D1,L1,P3,LD Sub XIV LLC\n\
r4,D1,L1,P4,Acme Coffee\n\
r5,D1,L1,P5,Acme Coffee Shop\n";

fn profile(cannot_link: &str) -> String {
    format!(
        r#"profile: cmbs_tenant_label
version: 0.1.0
entity_type: tenant_label
identity_semantics: canonical_display_label
canonical_type: tenant_label
required_fields:
  - source_row_id
  - deal_id
  - loan_id
  - property_id
  - raw_tenant_name
normalized_views:
  tenant_core:
    operators:
      - legal_basename
  tenant_tokens:
    operators:
      - legal_basename_tokens
  tenant_brand:
    operators:
      - legal_basename_fingerprint
prepare:
  primary_surface_fields: [raw_tenant_name]
  canonical_surface_normalized_view: tenant_core
  normalized_view_fields:
    tenant_core: raw_tenant_name
    tenant_tokens: raw_tenant_name
    tenant_brand: raw_tenant_name
  alias_surfaces_field: alias_surfaces_json
  mention_surfaces_field: mention_surfaces_json
  context_fields: [deal_id, loan_id, property_id]
  provenance_fields: [source_row_id, deal_id, loan_id, property_id]
evidence:
  support:
    - op: exact_view
      view: tenant_core
    - op: string_similarity
      view: tenant_core
      params:
        metric: jaro_winkler
        min_score_units: "9000"
    - op: tfidf_cosine
      view: tenant_tokens
      params:
        min_score_units: "1"
        top_k: "10"
        candidate_cap: "10"
  cannot_link:
{cannot_link}
  relation_hints:
    - op: related_brand_family
      view: tenant_brand
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: cmbs_tenant_label.aliases
  distinct: cmbs_tenant_label.distinct
  relations: cmbs_tenant_label.relations
"#
    )
}

const GUARD: &str = "    - op: protected_token_conflict\n      view: tenant_tokens";

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new(cannot_link: &str) -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        fs::create_dir_all(root.join("registry")).unwrap();
        fs::write(
            root.join("registry/registry.json"),
            r#"{"id":"t","version":"1.0.0","description":"t","updated":"2026-11-02","entry_count":1}"#,
        )
        .unwrap();
        fs::write(
            root.join("registry/aliases.json"),
            r#"[{"input":"Zed Holdings","canonical_id":"TNT-ZED","canonical_type":"tenant_label","rule_id":"T"}]"#,
        )
        .unwrap();
        fs::write(root.join("rows.csv"), ROWS).unwrap();
        fs::write(root.join("profile.yaml"), profile(cannot_link)).unwrap();
        Self { _temp: temp, root }
    }

    fn canon(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_canon"))
            .args(args)
            .current_dir(&self.root)
            .output()
            .expect("canon runs")
    }

    fn run(&self, work: &str) -> Output {
        self.canon(&[
            "entity",
            "run",
            "rows.csv",
            "--profile",
            "profile.yaml",
            "--strategy",
            "profile.yaml",
            "--registry",
            "registry",
            "--work-dir",
            work,
            "--emit",
            "summary",
            "--no-witness",
        ])
    }

    fn json(&self, relative: &str) -> Value {
        serde_json::from_slice(&fs::read(self.root.join(relative)).unwrap()).unwrap()
    }
}

fn parse(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON ({error}): {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn surface_names(fixture: &Fixture, work: &str) -> std::collections::BTreeMap<String, String> {
    fs::read_to_string(fixture.root.join(work).join("prepare/surfaces.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|surface| {
            (
                surface["surface_id"].as_str().unwrap().to_string(),
                surface["primary_surface"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

type Hits = Vec<(String, bool)>;

fn pair_hits(fixture: &Fixture, work: &str) -> Vec<(String, String, Hits)> {
    let names = surface_names(fixture, work);
    fs::read_to_string(fixture.root.join(work).join("evidence/evidence.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|record| {
            let hits = record["hits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|hit| {
                    (
                        hit["operator_id"].as_str().unwrap().to_string(),
                        hit["hard_cannot_link"].as_bool().unwrap(),
                    )
                })
                .collect();
            (
                names[record["left_surface_id"].as_str().unwrap()].clone(),
                names[record["right_surface_id"].as_str().unwrap()].clone(),
                hits,
            )
        })
        .collect()
}

fn is_cut(hits: &[(String, bool)]) -> bool {
    hits.iter()
        .any(|(op, hard)| *hard && op.starts_with("protected_token_conflict"))
}

#[test]
fn protected_token_conflict_cuts_numbered_siblings_and_leaves_name_variants_alone() {
    let fixture = Fixture::new(GUARD);
    let output = fixture.run("wd");
    assert!(output.status.success(), "run failed: {}", stderr(&output));

    let pairs = pair_hits(&fixture, "wd");
    let sub_pairs = pairs
        .iter()
        .filter(|(l, r, _)| l.contains("Sub") && r.contains("Sub"))
        .collect::<Vec<_>>();
    assert_eq!(sub_pairs.len(), 3, "III/IV/XIV form three candidate pairs");
    for (left, right, hits) in &sub_pairs {
        assert!(
            is_cut(hits),
            "{left} <> {right} must carry a hard cut: {hits:?}"
        );
    }
    // Negative: a naive "token sets differ" guard would also cut this pair,
    // because only one side has the extra word `shop`. It has no digit or
    // numeral, so it must stay uncut and keep its support evidence.
    let coffee = pairs
        .iter()
        .find(|(l, r, _)| l.contains("Acme") && r.contains("Acme"))
        .expect("Acme Coffee pair is a candidate");
    assert!(
        !is_cut(&coffee.2),
        "name variant must not be cut: {:?}",
        coffee.2
    );
    assert!(
        coffee
            .2
            .iter()
            .any(|(op, _)| op.starts_with("string_similarity"))
    );

    let solve = fixture.json("wd/solve/solve.json");
    let entities = solve["entities"].as_array().unwrap();
    let siblings = entities
        .iter()
        .find(|entity| entity["surface_ids"].as_array().unwrap().len() == 3)
        .expect("the three Sub vehicles still share one positive component");
    assert_eq!(siblings["state"], "contradiction");
    assert_eq!(siblings["reason"], "hard_cannot_link_constraint");
    assert!(siblings["hard_cannot_link_count"].as_u64().unwrap() >= 1);
    let acme = entities
        .iter()
        .find(|entity| entity["surface_ids"].as_array().unwrap().len() == 2)
        .expect("Acme Coffee variants component");
    assert_eq!(acme["hard_cannot_link_count"], 0);
    assert_ne!(acme["state"], "contradiction");
}

#[test]
fn without_the_operator_the_siblings_are_not_cut() {
    // Control: proves the cut above comes from the declared operator, not
    // from some other lane.
    let fixture = Fixture::new(
        "    - op: anchor_conflict\n      params:\n        field: lei\n        score_units: \"0\"",
    );
    let output = fixture.run("wd");
    assert!(output.status.success(), "run failed: {}", stderr(&output));
    assert!(
        pair_hits(&fixture, "wd")
            .iter()
            .all(|(_, _, hits)| !is_cut(hits))
    );
}

#[test]
fn protect_param_is_honored_and_validated() {
    let none = Fixture::new(&format!("{GUARD}\n      params:\n        protect: none"));
    assert!(none.run("wd").status.success());
    assert!(
        pair_hits(&none, "wd")
            .iter()
            .all(|(_, _, hits)| !is_cut(hits))
    );

    let listed = Fixture::new(&format!(
        "{GUARD}\n      params:\n        protect: none\n        protected_tokens: coffee,shop"
    ));
    assert!(listed.run("wd").status.success());
    let pairs = pair_hits(&listed, "wd");
    let acme = pairs.iter().find(|(l, _, _)| l.contains("Acme")).unwrap();
    assert!(
        is_cut(&acme.2),
        "listed token `shop` is protected: {:?}",
        acme.2
    );

    let bogus = Fixture::new(&format!("{GUARD}\n      params:\n        protect: letters"));
    let output = bogus.run("wd");
    assert_eq!(output.status.code(), Some(2));
    let refusal = stderr(&output);
    assert!(refusal.contains("E_ENTITY_ARTIFACT_CONTRACT"), "{refusal}");
    assert!(refusal.contains("protect"), "{refusal}");
    assert!(!bogus.root.join("wd/evidence/evidence.json").exists());

    let viewless = Fixture::new("    - op: protected_token_conflict");
    assert_eq!(viewless.run("wd").status.code(), Some(2));
}

#[test]
fn run_warns_about_declared_but_inert_operators_without_changing_the_outcome() {
    let inert = Fixture::new(&format!(
        "{GUARD}\n    - op: related_distinct_phrase\n      view: tenant_core"
    ));
    let output = inert.run("wd");
    assert!(output.status.success());
    let warning = stderr(&output);
    assert!(warning.contains("declared but inert"), "stderr: {warning}");
    assert!(warning.contains("cannot_link:related_distinct_phrase"));
    assert!(!warning.contains("cannot_link:protected_token_conflict"));
    // Declared-only ops never block a run: same artifact shape and exit code.
    assert!(String::from_utf8_lossy(&output.stdout).contains("canon_entity_run.v1"));

    // JSON mode is the machine channel: stderr stays empty.
    let json = inert.canon(&[
        "entity",
        "run",
        "rows.csv",
        "--profile",
        "profile.yaml",
        "--strategy",
        "profile.yaml",
        "--registry",
        "registry",
        "--work-dir",
        "wd-json",
        "--emit",
        "json",
        "--no-witness",
    ]);
    assert!(json.status.success());
    assert!(stderr(&json).is_empty(), "stderr: {}", stderr(&json));

    let clean = Fixture::new(GUARD);
    let output = clean.run("wd");
    assert!(
        !stderr(&output).contains("cannot_link:"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn run_review_export_matches_solve_export_and_imports_cleanly() {
    let fixture = Fixture::new(GUARD);
    assert!(fixture.run("wd").status.success());
    let groups = fixture.json("wd/solve/solve.json")["summary"]["counts"]["review_group_count"]
        .as_u64()
        .unwrap();
    assert!(groups >= 2, "fixture must produce several review groups");

    let from_run = fixture.canon(&[
        "entity",
        "review",
        "export",
        "wd/run/run.json",
        "--include",
        "all",
        "--emit",
        "json",
    ]);
    let from_solve = fixture.canon(&[
        "entity",
        "review",
        "export",
        "wd/solve/solve.json",
        "--include",
        "all",
        "--emit",
        "json",
    ]);
    let (run_queue, solve_queue) = (parse(&from_run), parse(&from_solve));
    let ids = |queue: &Value| {
        queue["review_items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["review_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let run_ids = ids(&run_queue);
    // One item per review group, not zero (run envelope) and not two
    // (entities[] + review_groups[] double count).
    assert_eq!(run_ids.len() as u64, groups, "{run_ids:?}");
    let unique = run_ids.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), run_ids.len(), "review ids must be unique");
    assert_eq!(run_ids, ids(&solve_queue));
    // The queue stays bound to the artifact it was exported from: audit and
    // promote certify the run, so the run export must not rebind to the solve.
    assert_eq!(
        run_queue["source_result"]["content_hash"],
        fixture.json("wd/run/run.json")["artifact_content_hash"]
    );
    assert_eq!(
        solve_queue["source_result"]["content_hash"],
        fixture.json("wd/solve/solve.json")["artifact_content_hash"]
    );
    assert_ne!(run_queue["source_result"], solve_queue["source_result"]);
    // The merged item keeps both records' information.
    let item = &run_queue["review_items"][0];
    assert!(
        item.get("reason").is_some() && item.get("review_group_id").is_some(),
        "{item}"
    );

    fs::write(fixture.root.join("queue.json"), &from_run.stdout).unwrap();
    fs::copy(
        fixture.root.join("registry/registry.json"),
        fixture.root.join("registry-before.json"),
    )
    .unwrap();
    let import = fixture.canon(&[
        "entity",
        "review",
        "import",
        "queue.json",
        "--registry",
        "registry",
        "--next-version",
        "1.0.1",
        "--emit",
        "summary",
    ]);
    let message = String::from_utf8_lossy(&import.stdout).into_owned() + &stderr(&import);
    assert!(import.status.success(), "import refused: {message}");
    assert!(!message.contains("duplicate review IDs"));
}

#[test]
fn run_export_refuses_a_mirrored_solve_that_does_not_match_the_run() {
    // Inside a work dir the committed publication stream wins over loose
    // files, so a swapped solve.json is simply ignored there. The hash check
    // matters for a mirror copied out without `.canon/` (direct file reads).
    let fixture = Fixture::new(GUARD);
    assert!(fixture.run("wd").status.success());
    fs::write(
        fixture.root.join("rows.csv"),
        format!("{ROWS}r6,D1,L1,P6,Other Name\n"),
    )
    .unwrap();
    assert!(fixture.run("other").status.success());
    for stage in ["run", "solve"] {
        fs::create_dir_all(fixture.root.join("mirror").join(stage)).unwrap();
        fs::copy(
            fixture
                .root
                .join("wd")
                .join(stage)
                .join(format!("{stage}.json")),
            fixture
                .root
                .join("mirror")
                .join(stage)
                .join(format!("{stage}.json")),
        )
        .unwrap();
    }
    let export = |fixture: &Fixture| {
        fixture.canon(&[
            "entity",
            "review",
            "export",
            "mirror/run/run.json",
            "--include",
            "all",
            "--emit",
            "json",
        ])
    };
    // Control: the untouched mirror exports, so the refusal below is caused
    // only by the swap.
    let control = export(&fixture);
    assert!(
        control.status.success(),
        "{}",
        String::from_utf8_lossy(&control.stdout)
    );
    assert!(
        !parse(&control)["review_items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    fs::copy(
        fixture.root.join("other/solve/solve.json"),
        fixture.root.join("mirror/solve/solve.json"),
    )
    .unwrap();
    let output = export(&fixture);
    assert_eq!(output.status.code(), Some(2));
    let refusal = &parse(&output)["refusal"];
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert!(
        refusal["message"]
            .as_str()
            .unwrap()
            .contains("does not match the run")
    );
}

#[test]
fn native_review_refusal_for_v1_artifacts_names_the_working_command() {
    let fixture = Fixture::new(GUARD);
    assert!(fixture.run("wd").status.success());
    for artifact in ["wd/run/run.json", "wd/solve/solve.json"] {
        let output = fixture.canon(&[
            "entity",
            "review",
            "export",
            artifact,
            "--artifact",
            "native-review",
            "--include",
            "all",
            "--emit",
            "json",
        ]);
        assert_eq!(output.status.code(), Some(2));
        let refusal = &parse(&output)["refusal"];
        assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
        let next = refusal["next_command"].as_str().unwrap();
        assert!(
            next.contains("canon entity review export <RESULT.json> --include all"),
            "{next}"
        );
    }
}

/// The operator catalog in the agent guide must match the validator's
/// accepted operators and the set `entity run` executes. Guards the observed
/// defect (bd-0al7/bd-dxfb): operators declarable in a profile that silently
/// do nothing. Retire if the catalog is generated from code instead.
#[test]
fn agent_guide_operator_catalog_matches_validator_and_run() {
    use canon::entity::{
        profile::supported_evidence_operators,
        run::{RUN_CANNOT_LINK_OPS, RUN_SUPPORT_OPS},
    };
    let guide = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/ENTITY_WORKBENCH_AGENT_GUIDE.md"),
    )
    .unwrap();
    let table = guide
        .split("<!-- operator-catalog:start -->")
        .nth(1)
        .and_then(|rest| rest.split("<!-- operator-catalog:end -->").next())
        .expect("catalog markers present");
    let documented = table
        .lines()
        .filter(|line| {
            line.starts_with("| ") && !line.starts_with("| Lane") && !line.contains("---")
        })
        .map(|line| {
            let cells = line
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect::<Vec<_>>();
            (
                cells[0].to_string(),
                cells[1].to_string(),
                cells[2] == "yes",
            )
        })
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for (lane, ops) in supported_evidence_operators() {
        let wired: &[&str] = match lane {
            "support" => RUN_SUPPORT_OPS,
            "cannot_link" => RUN_CANNOT_LINK_OPS,
            _ => &[],
        };
        for op in ops {
            expected.push((lane.to_string(), (*op).to_string(), wired.contains(op)));
        }
    }
    assert_eq!(
        documented, expected,
        "update the catalog table in the agent guide"
    );
}

#[test]
fn profile_refusals_name_the_supported_operators() {
    let fixture = Fixture::new("    - op: not_a_real_operator\n      view: tenant_tokens");
    let output = fixture.run("wd");
    assert_eq!(output.status.code(), Some(2));
    let refusal = stderr(&output);
    assert!(refusal.contains("supported_operators"), "{refusal}");
    assert!(refusal.contains("protected_token_conflict"), "{refusal}");
}

// ---- strategy `solve:` section (bd-i6o7 split, bd-vula standalone) ----

impl Fixture {
    fn run_with_strategy(&self, work: &str, solve_section: &str) -> Output {
        fs::write(
            self.root.join("strategy.yaml"),
            format!("strategy_id: solve_options\nstrategy_version: 0.1.0\n{solve_section}"),
        )
        .unwrap();
        self.canon(&[
            "entity",
            "run",
            "rows.csv",
            "--profile",
            "profile.yaml",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registry",
            "--work-dir",
            work,
            "--emit",
            "summary",
            "--no-witness",
        ])
    }
}

fn solve_entities(fixture: &Fixture, work: &str) -> Vec<(usize, String, String)> {
    let solve = fixture.json(&format!("{work}/solve/solve.json"));
    let mut entities = solve["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entity| {
            (
                entity["surface_ids"].as_array().unwrap().len(),
                entity["state"].as_str().unwrap().to_string(),
                entity["reason"].as_str().unwrap().to_string(),
            )
        })
        .collect::<Vec<_>>();
    entities.sort();
    entities
}

#[test]
fn solve_section_absent_keeps_the_contradiction_with_a_separate_strategy_file() {
    let fixture = Fixture::new(GUARD);
    let output = fixture.run_with_strategy("wd", "");
    assert!(output.status.success(), "{}", stderr(&output));
    let entities = solve_entities(&fixture, "wd");
    assert!(
        entities
            .iter()
            .any(|(n, state, _)| *n == 3 && state == "contradiction"),
        "{entities:?}"
    );
    let counts = &fixture.json("wd/solve/solve.json")["summary"]["counts"];
    assert!(counts.get("diagnostics_split_direct_cut_edges").is_none());
    assert!(counts.get("diagnostics_standalone_surfaces").is_none());
}

#[test]
fn split_direct_turns_the_sibling_contradiction_into_reviewable_singletons() {
    let fixture = Fixture::new(GUARD);
    let output = fixture.run_with_strategy("wd", "solve:\n  hard_cannot_link: split_direct\n");
    assert!(output.status.success(), "{}", stderr(&output));
    let entities = solve_entities(&fixture, "wd");
    assert!(
        entities
            .iter()
            .all(|(_, state, _)| state != "contradiction"),
        "{entities:?}"
    );
    // III / IV / XIV each stand alone; the Acme variants stay one component.
    assert_eq!(
        entities.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(),
        vec![1, 1, 1, 2],
        "{entities:?}"
    );
    assert_eq!(
        entities
            .iter()
            .filter(|(_, _, r)| r == "standalone_surface_no_support")
            .count(),
        3
    );
    let solve = fixture.json("wd/solve/solve.json");
    assert_eq!(
        solve["summary"]["counts"]["diagnostics_split_direct_cut_edges"],
        3
    );
    assert_eq!(solve["summary"]["counts"]["contradictions"], 0);
    // The cut is auditable: the evidence artifact still records both the
    // support and the hard cannot-link on each cut pair.
    let cut_pairs = pair_hits(&fixture, "wd")
        .into_iter()
        .filter(|(l, r, _)| l.contains("Sub") && r.contains("Sub"))
        .collect::<Vec<_>>();
    assert_eq!(cut_pairs.len(), 3);
    assert!(cut_pairs.iter().all(|(_, _, hits)| {
        is_cut(hits)
            && hits
                .iter()
                .any(|(op, _)| op.starts_with("string_similarity"))
    }));

    // Every singleton is now a reviewable item that survives export + import.
    let export = fixture.canon(&[
        "entity",
        "review",
        "export",
        "wd/run/run.json",
        "--include",
        "all",
        "--emit",
        "json",
    ]);
    let queue = parse(&export);
    assert_eq!(queue["review_items"].as_array().unwrap().len(), 4);
    fs::write(fixture.root.join("queue.json"), &export.stdout).unwrap();
    let import = fixture.canon(&[
        "entity",
        "review",
        "import",
        "queue.json",
        "--registry",
        "registry",
        "--next-version",
        "1.0.1",
        "--emit",
        "summary",
    ]);
    assert!(import.status.success(), "{}", stderr(&import));
}

#[test]
fn standalone_surfaces_are_listed_only_when_the_strategy_includes_them() {
    let fixture = Fixture::new(GUARD);
    fs::write(
        fixture.root.join("rows.csv"),
        format!(
            "{ROWS}r6,D1,L1,P6,Zephyr Quantum Logistics\nr7,D1,L1,P7,Obsidian Harbor Partners\n"
        ),
    )
    .unwrap();
    assert!(fixture.run_with_strategy("omit", "").status.success());
    let omitted = solve_entities(&fixture, "omit");
    assert_eq!(
        omitted.iter().map(|(n, _, _)| n).sum::<usize>(),
        5,
        "{omitted:?}"
    );

    assert!(
        fixture
            .run_with_strategy("incl", "solve:\n  standalone_surfaces: include\n")
            .status
            .success()
    );
    let included = solve_entities(&fixture, "incl");
    assert_eq!(
        included.iter().map(|(n, _, _)| n).sum::<usize>(),
        7,
        "{included:?}"
    );
    let lone = included
        .iter()
        .filter(|(n, state, reason)| {
            *n == 1 && state == "escrow" && reason == "standalone_surface_no_support"
        })
        .count();
    assert_eq!(lone, 2, "{included:?}");
    assert_eq!(
        fixture.json("incl/solve/solve.json")["summary"]["counts"]["diagnostics_standalone_surfaces"],
        2
    );
    // Never promotable: a lone surface cannot reach PromotableNew.
    assert_eq!(
        fixture.json("incl/solve/solve.json")["summary"]["counts"]["promotable_new"],
        0
    );
}

#[test]
fn invalid_solve_section_refuses_before_any_solve_output() {
    for (index, section) in [
        "solve:\n  hard_cannot_link: yolo\n",
        "solve:\n  unknown_setting: include\n",
        "solve: split_direct\n",
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = Fixture::new(GUARD);
        let work = format!("wd{index}");
        let output = fixture.run_with_strategy(&work, section);
        assert_eq!(output.status.code(), Some(2), "{section}");
        let message = stderr(&output);
        assert!(
            message.contains("unsupported_solve_setting")
                || message.contains("solve_section_must_be_a_mapping"),
            "{message}"
        );
        assert!(!fixture.root.join(&work).join("solve/solve.json").exists());
    }
}
