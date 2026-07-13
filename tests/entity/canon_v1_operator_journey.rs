#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

const PROJECT_TOML: &str = include_str!("../fixtures/canon_v1/operator_journey/project.toml");
const OBSERVATIONS_CSV: &str =
    include_str!("../fixtures/canon_v1/operator_journey/observations.csv");
const EXPECTED_REVIEW_JSON: &str =
    include_str!("../fixtures/canon_v1/operator_journey/expected_review.json");
const STRATEGY_YAML: &str = include_str!("../../rules/golden_rules.yaml");

#[test]
#[ignore = "bd-1hum: requires real public evidence/solve v1 stage dependencies; current journey still uses a synthetic v1 bridge"]
fn canon_v1_operator_journey_uses_one_public_binary_and_exact_replay() {
    assert_fixture_contracts_are_neutral_and_bounded();

    let first = run_operator_journey("first");
    let second = run_operator_journey("second");
    assert_eq!(
        second.projection, first.projection,
        "journey projections must be deterministic across fresh work dirs"
    );

    assert_eq!(first.projection.link_summary["matched"], 1);
    assert_eq!(first.projection.link_summary["ambiguous"], 1);
    assert_eq!(first.projection.link_summary["unmatched"], 1);
    assert_eq!(first.projection.promoted_aliases, 2);
    assert_eq!(first.projection.exact_lookup_outcome, "RESOLVED");
    assert_eq!(first.projection.apply_resolved, 3);
    assert_eq!(first.projection.dbt_alias_rows, 3);
    assert_eq!(first.projection.search_alias_rows, 3);
    assert!(first.projection.legacy_engine_refused);

    assert!(
        first.command_log.len() >= 20,
        "acceptance records each public binary stage"
    );
    for record in &first.command_log {
        assert_eq!(record.binary, "canon");
        assert!(
            !record.command_line.contains(" python ")
                && !record.command_line.contains(" curl ")
                && !record.command_line.contains(" http://")
                && !record.command_line.contains(" https://"),
            "operator journey command must stay local: {}",
            record.command_line
        );
        assert!(
            record.stdout_json
                || record.stdout_csv
                || record.stdout_text
                || !record.stdout.is_empty()
                || !record.stderr.is_empty(),
            "command records stdout/stderr routing: {}",
            record.command_line
        );
    }
}

#[derive(Debug)]
struct JourneyRun {
    projection: JourneyProjection,
    command_log: Vec<CommandRecord>,
}

#[derive(Debug, PartialEq, Eq)]
struct JourneyProjection {
    run_counts: BTreeMap<String, u64>,
    link_summary: BTreeMap<String, u64>,
    review_states: Vec<String>,
    review_priority_reasons: Vec<String>,
    native_review_modes: Vec<String>,
    promoted_aliases: u64,
    exact_lookup_outcome: String,
    apply_resolved: u64,
    dbt_alias_rows: usize,
    search_alias_rows: usize,
    legacy_engine_refused: bool,
}

#[derive(Debug)]
struct CommandRecord {
    binary: &'static str,
    command_line: String,
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_json: bool,
    stdout_csv: bool,
    stdout_text: bool,
}

struct JourneyFixture {
    _temp: TempDir,
    root: PathBuf,
    observations: PathBuf,
    reference: PathBuf,
    target: PathBuf,
    strategy: PathBuf,
    profile: PathBuf,
    registry: PathBuf,
    suite: PathBuf,
}

fn run_operator_journey(label: &str) -> JourneyRun {
    let fixture = JourneyFixture::new(label);
    let mut command_log = Vec::new();

    let profile_list = canon_json(&["entity", "profile", "list", "--emit", "json"], 0);
    command_log.push(profile_list.record);
    assert!(
        profile_list.value["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|profile| profile["profile"] == "regab_firm_identity")
    );

    let profile_init = canon_json(
        &[
            "entity",
            "profile",
            "init",
            "regab_firm_identity",
            "--output",
            path_str(&fixture.profile),
        ],
        0,
    );
    command_log.push(profile_init.record);
    assert_eq!(profile_init.value["template_valid"], true);
    assert!(
        fs::read_to_string(&fixture.profile)
            .unwrap()
            .contains("same_firm_or_reviewed_alias")
    );

    let stage_work = fixture.root.join("stage-work");
    let prepare = canon_json(
        &[
            "entity",
            "prepare",
            path_str(&fixture.observations),
            "--profile",
            path_str(&fixture.profile),
            "--registry",
            path_str(&fixture.registry),
            "--work-dir",
            path_str(&stage_work),
        ],
        0,
    );
    command_log.push(prepare.record);
    assert_eq!(prepare.value["version"], "canon_entity_prepare.v0");

    let index = canon_json(
        &[
            "entity",
            "index",
            "build",
            path_str(&fixture.observations),
            "--profile",
            path_str(&fixture.profile),
            "--strategy",
            path_str(&fixture.strategy),
            "--registry",
            path_str(&fixture.registry),
            "--work-dir",
            path_str(&stage_work),
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(index.record);
    assert_eq!(index.value["version"], "canon_entity_index_build.v0");

    let block = canon_raw(
        &[
            "entity",
            "block",
            path_str(&fixture.observations),
            "--profile",
            path_str(&fixture.profile),
            "--strategy",
            path_str(&fixture.strategy),
            "--registry",
            path_str(&fixture.registry),
            "--work-dir",
            path_str(&stage_work),
            "--emit",
            "jsonl",
        ],
        0,
    );
    assert!(
        !block.stdout.is_empty(),
        "manual block emits candidate JSONL"
    );
    command_log.push(block);

    let run = canon_json(
        &[
            "entity",
            "run",
            path_str(&fixture.observations),
            "--profile",
            path_str(&fixture.profile),
            "--strategy",
            path_str(&fixture.strategy),
            "--registry",
            path_str(&fixture.registry),
            "--work-dir",
            path_str(&stage_work),
            "--cache-mode",
            "disabled",
            "--no-witness",
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(run.record);
    assert_eq!(run.value["version"], "canon_entity_run.v0");
    assert_eq!(read_json(&stage_work.join("run.json")), run.value);
    assert_bound_stage_artifact(&run.value, &stage_work.join("block/block.json"), "block");
    assert_nonempty_file(&stage_work.join("block/candidates.jsonl"));
    assert_bound_stage_artifact(&run.value, &stage_work.join("edge/edge.json"), "edge");
    assert_nonempty_file(&stage_work.join("edge/edges.jsonl"));
    assert_bound_stage_artifact(&run.value, &stage_work.join("solve/solve.json"), "solve");
    assert_next_commands_are_public_and_local(&run.value["next_commands"]);

    let link_work = fixture.root.join("link-work");
    let link = canon_json(
        &[
            "entity",
            "link",
            path_str(&fixture.reference),
            path_str(&fixture.target),
            "--profile",
            path_str(&fixture.profile),
            "--strategy",
            path_str(&fixture.strategy),
            "--registry",
            path_str(&fixture.registry),
            "--work-dir",
            path_str(&link_work),
            "--cache-mode",
            "disabled",
            "--no-witness",
            "--emit",
            "json",
        ],
        1,
    );
    command_log.push(link.record);
    assert_eq!(link.value["version"], "canon_entity_link.v0");
    assert_eq!(
        link.value["shared_solve_artifact"]["content_hash"],
        read_json(&link_work.join("solve/solve.json"))["artifact_content_hash"]
    );
    assert_next_commands_are_public_and_local(&link.value["next_commands"]);

    let audit = canon_json(
        &[
            "entity",
            "audit",
            path_str(&link_work.join("run.json")),
            "--suite",
            path_str(&fixture.suite),
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(audit.record);
    assert_eq!(audit.value["version"], "canon_entity_audit.v0");
    assert_eq!(audit.value["summary"]["labels"]["status"], "passed");

    let queue_review = canon_json(
        &[
            "entity",
            "review",
            "export",
            path_str(&link_work.join("link/link.json")),
            "--include",
            "all",
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(queue_review.record);
    assert_eq!(
        queue_review.value["version"],
        "canon_entity_review_queue.v0"
    );
    assert_review_projection(&queue_review.value);

    let native_review = canon_json(
        &[
            "entity",
            "review",
            "export",
            path_str(&link_work.join("link/link.json")),
            "--artifact",
            "native-review",
            "--include",
            "all",
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(native_review.record);
    assert_eq!(
        native_review.value["version"],
        "canon_entity_native_review.v0"
    );

    let native_review_path = fixture.root.join("native-review.json");
    write_json(&native_review_path, &native_review.value);
    let decisions_path = fixture.root.join("native-review-decisions.json");
    write_json(
        &decisions_path,
        &json!({
            "decisions": native_review.value["review_items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| native_defer_decision(&native_review.value, item))
                .collect::<Vec<_>>()
        }),
    );
    let registry_before_import_refusal = registry_snapshot(&fixture.registry);
    let import = canon_json(
        &[
            "entity",
            "review",
            "import",
            path_str(&decisions_path),
            "--registry",
            path_str(&fixture.registry),
            "--next-version",
            "0.1.1",
            "--source-review",
            path_str(&native_review_path),
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(import.record);
    assert_eq!(
        import.value["version"],
        "canon_entity_native_review_import.v0"
    );
    assert_eq!(
        registry_snapshot(&fixture.registry),
        registry_before_import_refusal,
        "native review import is a typed receipt and must not mutate registry"
    );

    let stale_decisions_path = fixture.root.join("native-review-decisions-stale.json");
    let mut stale_decisions = read_json(&decisions_path);
    stale_decisions["decisions"][0]["source_review_artifact_hash"] =
        Value::String("blake3:stale".to_string());
    write_json(&stale_decisions_path, &stale_decisions);
    let stale_import = canon_json(
        &[
            "entity",
            "review",
            "import",
            path_str(&stale_decisions_path),
            "--registry",
            path_str(&fixture.registry),
            "--next-version",
            "0.1.1",
            "--source-review",
            path_str(&native_review_path),
            "--emit",
            "json",
        ],
        2,
    );
    command_log.push(stale_import.record);
    assert_eq!(
        stale_import.value["refusal"]["code"],
        "E_ENTITY_REVIEW_IMPORT"
    );
    assert_eq!(
        stale_import.value["refusal"]["detail"]["writes_performed"],
        false
    );
    assert_eq!(
        registry_snapshot(&fixture.registry),
        registry_before_import_refusal,
        "stale review refusal must be atomic"
    );

    let native_promote_refusal = canon_json(
        &[
            "entity",
            "promote",
            path_str(&link_work.join("run.json")),
            "--audit",
            path_str(&fixture.root.join("audit.json")),
            "--registry",
            path_str(&fixture.registry),
            "--next-version",
            "0.1.1",
            "--emit",
            "json",
        ],
        2,
    );
    command_log.push(native_promote_refusal.record);
    assert_eq!(
        registry_snapshot(&fixture.registry),
        registry_before_import_refusal
    );

    let v1_result_path = fixture.root.join("promotable-run.v1.json");
    let v1_result = build_v1_run_artifact(
        &run.value,
        &fixture,
        "0.1.0",
        registry_snapshot_hash_for_apply(&fixture.registry),
        vec![
            ("Harbor Metrics", "ORG-0002"),
            ("Quartz Signal", "ORG-0003"),
        ],
    );
    write_json(&v1_result_path, &v1_result);
    let v1_audit = canon_json(
        &[
            "entity",
            "audit",
            path_str(&v1_result_path),
            "--suite",
            path_str(&fixture.suite),
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(v1_audit.record);
    let v1_audit_path = fixture.root.join("promotable-audit.v1.json");
    write_json(&v1_audit_path, &v1_audit.value);

    let tampered_audit_path = fixture.root.join("promotable-audit-tampered.v1.json");
    let mut tampered_audit = v1_audit.value.clone();
    tampered_audit["summary"]["labels"]["status"] = Value::String("failed".to_string());
    write_json(&tampered_audit_path, &tampered_audit);
    let before_tampered_promote = registry_snapshot(&fixture.registry);
    let tampered_promote = canon_json(
        &[
            "entity",
            "promote",
            path_str(&v1_result_path),
            "--audit",
            path_str(&tampered_audit_path),
            "--registry",
            path_str(&fixture.registry),
            "--next-version",
            "0.1.1",
            "--emit",
            "json",
        ],
        2,
    );
    command_log.push(tampered_promote.record);
    assert_eq!(
        tampered_promote.value["refusal"]["code"],
        "E_ENTITY_AUDIT_GATE"
    );
    assert_eq!(
        registry_snapshot(&fixture.registry),
        before_tampered_promote,
        "tampered audit refusal must not mutate registry"
    );

    let promote = canon_json(
        &[
            "entity",
            "promote",
            path_str(&v1_result_path),
            "--audit",
            path_str(&v1_audit_path),
            "--registry",
            path_str(&fixture.registry),
            "--next-version",
            "0.1.1",
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(promote.record);
    assert_eq!(promote.value["version"], "canon_entity_promote.v1");
    assert_eq!(
        read_json(&fixture.registry.join("registry.json"))["version"],
        "0.1.1"
    );

    let lookup_input = fixture.root.join("lookup.csv");
    fs::write(
        &lookup_input,
        "org_name\nNorthstar Analytics\nHarbor Metrics\nQuartz Signal\n",
    )
    .unwrap();
    let exact_lookup = canon_json(
        &[
            path_str(&lookup_input),
            "--registry",
            path_str(&fixture.registry),
            "--column",
            "org_name",
            "--emit",
            "json",
            "--explicit",
            "--no-witness",
        ],
        0,
    );
    command_log.push(exact_lookup.record);
    assert_eq!(exact_lookup.value["version"], "canon.v0");
    assert_eq!(exact_lookup.value["outcome"], "RESOLVED");

    let replay_result_path = fixture.root.join("replay-run.v1.json");
    let replay_result = build_v1_run_artifact(
        &run.value,
        &fixture,
        "0.1.1",
        registry_snapshot_hash_for_apply(&fixture.registry),
        Vec::new(),
    );
    write_json(&replay_result_path, &replay_result);
    let apply_out = fixture.root.join("applied.csv");
    let apply = canon_json(
        &[
            "entity",
            "apply",
            path_str(&replay_result_path),
            "--rows",
            path_str(&fixture.observations),
            "--registry",
            path_str(&fixture.registry),
            "--column",
            "org_name",
            "--output",
            path_str(&apply_out),
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(apply.record);
    assert_eq!(apply.value["version"], "canon_entity_apply.v0");
    assert!(fs::read_to_string(&apply_out).unwrap().contains(
        "org_name,bucket,canonical_id,canonical_type,canonical_status,canonical_registry_id"
    ));

    let explain = canon_json(
        &[
            "entity",
            "explain",
            path_str(&replay_result_path),
            "--canon-id",
            "ORG-0002",
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(explain.record);
    assert_eq!(explain.value["version"], "canon_entity_explain.v1");
    assert_eq!(explain.value["summary"]["counts"]["selected_records"], 1);

    let dbt_seed = fixture.root.join("registry_seed.csv");
    let dbt_schema = fixture.root.join("schema.yml");
    let dbt_test = fixture.root.join("assert_no_collapse.sql");
    let before_export = registry_snapshot(&fixture.registry);
    let dbt = canon_json(
        &[
            "registry",
            "export",
            "--format",
            "dbt-seed",
            "--registry",
            path_str(&fixture.registry),
            "--namespace",
            "canon_v1_operator_journey",
            "--out",
            path_str(&dbt_seed),
            "--schema-out",
            path_str(&dbt_schema),
            "--anti-collapse-test-out",
            path_str(&dbt_test),
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(dbt.record);
    assert_eq!(registry_snapshot(&fixture.registry), before_export);

    let search_db = fixture.root.join("registry.search.sqlite");
    let search = canon_json(
        &[
            "registry",
            "export",
            "--format",
            "search-index",
            "--registry",
            path_str(&fixture.registry),
            "--namespace",
            "canon_v1_operator_journey",
            "--out",
            path_str(&search_db),
            "--emit",
            "json",
        ],
        0,
    );
    command_log.push(search.record);
    assert_eq!(registry_snapshot(&fixture.registry), before_export);

    let describe = canon_json(&["--describe"], 0);
    command_log.push(describe.record);
    let legacy_engine_refused = !describe.value["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "resolve" || entry["name"] == "org");
    let legacy_cli = canon_raw(&["resolve", "--help"], 2);
    command_log.push(legacy_cli);

    JourneyRun {
        projection: JourneyProjection {
            run_counts: counts(&run.value["summary"]["counts"]),
            link_summary: counts(&link.value["summary"]),
            review_states: review_states(&queue_review.value),
            review_priority_reasons: review_priority_reasons(&queue_review.value),
            native_review_modes: native_review_modes(&native_review.value),
            promoted_aliases: promote.value["summary"]["counts"]["promoted_aliases"]
                .as_u64()
                .unwrap(),
            exact_lookup_outcome: exact_lookup.value["outcome"].as_str().unwrap().to_string(),
            apply_resolved: apply.value["summary"]["resolved"].as_u64().unwrap(),
            dbt_alias_rows: csv_data_row_count(&fs::read_to_string(&dbt_seed).unwrap()),
            search_alias_rows: sqlite_alias_count(&search_db),
            legacy_engine_refused,
        },
        command_log,
    }
}

impl JourneyFixture {
    fn new(label: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(label);
        fs::create_dir_all(&root).unwrap();
        let observations = root.join("observations.csv");
        let reference = root.join("reference.csv");
        let target = root.join("target.csv");
        let strategy = root.join("golden_rules.yaml");
        let profile = root.join("regab_firm_identity.yaml");
        let registry = root.join("registry");
        let suite = root.join("suite");
        fs::create_dir_all(&registry).unwrap();
        fs::create_dir_all(&suite).unwrap();

        fs::write(&observations, OBSERVATIONS_CSV).unwrap();
        fs::write(&strategy, STRATEGY_YAML).unwrap();
        split_observations(&observations, &reference, &target);
        write_registry(&registry, "0.1.0", 1);
        fs::write(
            registry.join("aliases.json"),
            serde_json::to_vec_pretty(&json!([
                {
                    "input": "Northstar Analytics",
                    "canonical_id": "ORG-0001",
                    "canonical_type": "org",
                    "rule_id": "SEED_ALIAS"
                }
            ]))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            suite.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "id": "canon_v1_operator_journey_suite",
                "version": "2026.07.13",
                "gates": [
                    {
                        "gate_id": "G01",
                        "label": "artifact continuity",
                        "passed": true,
                        "expected": "native_and_v1_artifacts_validate",
                        "actual": "native_and_v1_artifacts_validate",
                        "evidence": {
                            "bead": "bd-1hum"
                        }
                    },
                    {
                        "gate_id": "G14",
                        "label": "promotion preflight",
                        "passed": true,
                        "expected": "reviewed_aliases_bound",
                        "actual": "reviewed_aliases_bound",
                        "evidence": {
                            "bead": "bd-1hum"
                        }
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        Self {
            _temp: temp,
            root,
            observations,
            reference,
            target,
            strategy,
            profile,
            registry,
            suite,
        }
    }
}

struct JsonCommand {
    value: Value,
    record: CommandRecord,
}

fn canon_json(args: &[&str], expected_code: i32) -> JsonCommand {
    let record = canon_raw(args, expected_code);
    let value = serde_json::from_slice(&record.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout must be JSON for `{}`: {error}\nstdout={}\nstderr={}",
            record.command_line,
            String::from_utf8_lossy(&record.stdout),
            String::from_utf8_lossy(&record.stderr)
        )
    });
    JsonCommand { value, record }
}

fn canon_raw(args: &[&str], expected_code: i32) -> CommandRecord {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .unwrap();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout_json = serde_json::from_slice::<Value>(&output.stdout).is_ok();
    let stdout_csv = output
        .stdout
        .split(|byte| *byte == b'\n')
        .next()
        .is_some_and(|line| line.contains(&b','))
        && !stdout_json;
    let stdout_text = !output.stdout.is_empty() && !stdout_json && !stdout_csv;
    let record = CommandRecord {
        binary: "canon",
        command_line: format!("canon {}", args.join(" ")),
        exit_code,
        stdout: output.stdout,
        stderr: output.stderr,
        stdout_json,
        stdout_csv,
        stdout_text,
    };
    assert_eq!(
        record.exit_code,
        expected_code,
        "unexpected exit for `{}`\nstdout={}\nstderr={}",
        record.command_line,
        String::from_utf8_lossy(&record.stdout),
        String::from_utf8_lossy(&record.stderr)
    );
    record
}

fn split_observations(observations: &Path, reference: &Path, target: &Path) {
    let mut reader = csv::Reader::from_path(observations).unwrap();
    let headers = reader.headers().unwrap().clone();
    let mut ref_writer = csv::Writer::from_path(reference).unwrap();
    let mut target_writer = csv::Writer::from_path(target).unwrap();
    ref_writer.write_record(&headers).unwrap();
    target_writer.write_record(&headers).unwrap();
    let dataset_idx = headers
        .iter()
        .position(|header| header == "dataset")
        .unwrap();
    for record in reader.records() {
        let record = record.unwrap();
        match record.get(dataset_idx).unwrap() {
            "reference" => ref_writer.write_record(&record).unwrap(),
            "target" => target_writer.write_record(&record).unwrap(),
            other => panic!("unexpected dataset {other}"),
        }
    }
    ref_writer.flush().unwrap();
    target_writer.flush().unwrap();
}

fn write_registry(registry: &Path, version: &str, entry_count: u64) {
    fs::write(
        registry.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "canon-v1-neutral",
            "version": version,
            "description": "Neutral Canon v1 operator journey registry",
            "updated": "2026-07-13",
            "entry_count": entry_count
        }))
        .unwrap(),
    )
    .unwrap();
}

fn native_defer_decision(review: &Value, item: &Value) -> Value {
    json!({
        "review_id": item["review_id"],
        "mode": item["mode"],
        "action": "defer",
        "operator_id": "bd-1hum-acceptance",
        "reason_code": "acceptance_defer",
        "source_review_artifact_hash": review["artifact_content_hash"],
        "decision_binding_hash": item["decision_binding_hash"],
        "run_content_hash": review["binding"]["run_content_hash"],
        "policy_content_hash": review["binding"]["policy_content_hash"],
        "registry_snapshot_hash": review["binding"]["registry_snapshot_hash"],
        "mode_context": item["mode_context"].clone()
    })
}

fn build_v1_run_artifact(
    source_run: &Value,
    fixture: &JourneyFixture,
    registry_version: &str,
    registry_snapshot_hash: String,
    promotion_aliases: Vec<(&str, &str)>,
) -> Value {
    let mut metadata = source_run["metadata"].clone();
    metadata["schema"] = json!({
        "key": "canon_entity_run.v1",
        "content_hash": "blake3:schema-run"
    });
    metadata["registry_snapshot"]["version"] = Value::String(registry_version.to_string());
    metadata["registry_snapshot"]["source"] = Value::String(fixture.registry.display().to_string());
    metadata["registry_snapshot"]["lookup_snapshot_hash"] = Value::String(registry_snapshot_hash);
    metadata["workdir"] = json!({
        "root_dir": fixture.root.display().to_string(),
        "stage_dir": "run",
        "artifact_relpath": "run/run.json",
        "payload_relpath": "run/manifest.json"
    });
    metadata["upstream_artifacts"] = Value::Array(Vec::new());
    metadata["artifact_content_hash"] = Value::String(String::new());

    let aliases = promotion_aliases
        .iter()
        .map(|(input, canonical_id)| {
            json!({
                "input": input,
                "canonical_id": canonical_id,
                "canonical_type": "org",
                "rule_id": "ENTITY_V1_PROMOTE"
            })
        })
        .collect::<Vec<_>>();
    let entities = vec![
        json!({
            "canonical_id": "ORG-0002",
            "canonical_type": "org",
            "alias_inputs": ["Harbor Metrics"],
            "surface_id": "surface:harbor",
            "row_id": "tgt-amb"
        }),
        json!({
            "canonical_id": "ORG-0003",
            "canonical_type": "org",
            "alias_inputs": ["Quartz Signal"],
            "surface_id": "surface:quartz",
            "row_id": "tgt-none"
        }),
    ];
    let mut artifact = json!({
        "version": "canon_entity_run.v1",
        "artifact_content_hash": "",
        "metadata": metadata,
        "summary": {
            "counts": {
                "entities": entities.len() as u64,
                "promotable_aliases": aliases.len() as u64
            },
            "labels": {
                "stage": "run",
                "status": "completed"
            }
        },
        "run_manifest_path": "run/manifest.json",
        "promotable_aliases": aliases,
        "entities": entities,
        "rows": [
            {
                "row_id": "tgt-amb",
                "surface_id": "surface:harbor",
                "canonical_id": "ORG-0002"
            },
            {
                "row_id": "tgt-none",
                "surface_id": "surface:quartz",
                "canonical_id": "ORG-0003"
            }
        ],
        "evidence": [],
        "review_items": []
    });
    set_v1_hash(&mut artifact);
    artifact
}

fn set_v1_hash(artifact: &mut Value) {
    artifact["artifact_content_hash"] = Value::String("blake3:placeholder".to_string());
    artifact["metadata"]["artifact_content_hash"] = Value::String("blake3:placeholder".to_string());
    let hash = canon::entity::schema::compute_entity_v1_self_hash(artifact).unwrap();
    artifact["artifact_content_hash"] = Value::String(hash.clone());
    artifact["metadata"]["artifact_content_hash"] = Value::String(hash);
}

fn registry_snapshot_hash_for_apply(registry: &Path) -> String {
    let mut files = vec![registry.join("registry.json")];
    for entry in fs::read_dir(registry).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file()
            && path.extension().and_then(OsStr::to_str) == Some("json")
            && path.file_name().and_then(OsStr::to_str) != Some("registry.json")
            && path.file_name().and_then(OsStr::to_str) != Some("_build.json")
        {
            files.push(path);
        }
    }
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        hasher.update(path.file_name().unwrap().to_string_lossy().as_bytes());
        hasher.update(&[0]);
        hasher.update(&fs::read(path).unwrap());
        hasher.update(&[0xff]);
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn registry_snapshot(registry: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    for entry in fs::read_dir(registry).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            snapshot.insert(
                entry.file_name().to_string_lossy().to_string(),
                fs::read(path).unwrap(),
            );
        }
    }
    snapshot
}

fn assert_fixture_contracts_are_neutral_and_bounded() {
    assert!(PROJECT_TOML.contains("schema_version = \"canon_v1_operator_journey_project.v0\""));
    assert!(PROJECT_TOML.contains("legacy_engine_selection = \"forbidden\""));
    assert!(OBSERVATIONS_CSV.contains("Northstar Analytics"));
    assert!(STRATEGY_YAML.contains("strategy_id: canon-v1-operator-journey.v1"));
    for forbidden in [
        "cmbs", "dera", "figi", "openfigi", "python", "http://", "https://",
    ] {
        assert!(
            !OBSERVATIONS_CSV.to_ascii_lowercase().contains(forbidden),
            "neutral fixture must not contain {forbidden}"
        );
    }
}

fn assert_review_projection(review: &Value) {
    let expected: Value = serde_json::from_str(EXPECTED_REVIEW_JSON).unwrap();
    assert_eq!(review["version"], expected["queue_review"]["version"]);
    assert_eq!(
        review_states(review),
        expected["queue_review"]["states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        review_priority_reasons(review),
        expected["queue_review"]["priority_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );
}

fn assert_next_commands_are_public_and_local(next_commands: &Value) {
    for command in next_commands
        .as_object()
        .unwrap()
        .values()
        .filter_map(Value::as_str)
    {
        assert!(command.starts_with("canon entity "));
        assert!(!command.contains("python"));
        assert!(!command.contains("http://"));
        assert!(!command.contains("https://"));
    }
}

fn assert_bound_stage_artifact(run: &Value, path: &Path, stage: &str) {
    let artifact = read_json(path);
    let hash = artifact["artifact_content_hash"].as_str().unwrap();
    assert!(
        hash.starts_with("blake3:"),
        "{stage} artifact must carry a BLAKE3 self hash"
    );
    assert_eq!(artifact["metadata"]["artifact_content_hash"], hash);
    assert!(
        run["stage_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["stage"] == stage
                && entry["artifact_content_hash"] == hash
                && entry["path"]
                    == path
                        .strip_prefix(path.parent().unwrap().parent().unwrap())
                        .unwrap()
                        .to_str()
                        .unwrap()),
        "run artifact must bind the {stage} stage artifact"
    );
    assert!(
        run["metadata"]["upstream_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["version"] == artifact["version"] && entry["content_hash"] == hash),
        "run metadata must bind the {stage} stage artifact"
    );
}

fn assert_nonempty_file(path: &Path) {
    assert!(
        fs::metadata(path).unwrap().len() > 0,
        "{} is empty",
        path.display()
    );
}

fn counts(value: &Value) -> BTreeMap<String, u64> {
    value
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(key, value)| value.as_u64().map(|count| (key.clone(), count)))
        .collect()
}

fn review_states(review: &Value) -> Vec<String> {
    let mut states = review["review_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["state"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    states.sort();
    states
}

fn review_priority_reasons(review: &Value) -> Vec<String> {
    let mut reasons = BTreeSet::new();
    for item in review["review_items"].as_array().unwrap() {
        for reason in item["priority_reasons"].as_array().unwrap() {
            reasons.insert(reason.as_str().unwrap().to_string());
        }
    }
    reasons.into_iter().collect()
}

fn native_review_modes(review: &Value) -> Vec<String> {
    let mut modes = review["review_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["mode"].as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    modes.sort();
    modes
}

fn csv_data_row_count(csv: &str) -> usize {
    csv.lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        .saturating_sub(1)
}

fn sqlite_alias_count(path: &Path) -> usize {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    conn.query_row("SELECT COUNT(*) FROM aliases", [], |row| {
        row.get::<_, i64>(0)
    })
    .unwrap() as usize
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}
