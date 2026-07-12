use assert_cmd::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use tempfile::tempdir;

mod common;
use common::{canon_std_command_in_manifest, copy_json_registry_fixture};

const UNCHANGED_REFERENCE_TAPE: &str =
    "tests/fixtures/resolve/parity/unchanged-link/reference_loans.link.csv";
const UNCHANGED_TARGET_TAPE: &str =
    "tests/fixtures/resolve/parity/unchanged-link/target_loans.link.csv";
const CMBS_STRATEGY: &str = "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml";
const LOAN_MATCH_GOLD: &str = "tests/fixtures/resolve/gold/loan_matches.jsonl";
const UNCHANGED_MANIFEST: &str = "tests/fixtures/resolve/parity/unchanged-link/manifest.json";
const UNCHANGED_SOURCE_MAPPING: &str =
    "tests/fixtures/resolve/parity/unchanged-link/source_mapping.canon.source.mapping.v1.json";
const UNCHANGED_DECISION_GOLDEN: &str =
    "tests/fixtures/resolve/golden/unchanged_input_decision_projection.json";

#[test]
fn full_fixture_corpus_resolves_expected_records() {
    let temp_dir = tempdir().unwrap();
    let payload = run_full_entity_link_json(temp_dir.path(), 1, &[]);
    let decisions = &payload["decision_artifact"];

    assert_eq!(payload["version"], "canon_entity_link.v0");
    assert_eq!(payload["summary"]["target_records"], 12);
    assert_eq!(payload["summary"]["matched"], 9);
    assert_eq!(payload["summary"]["unmatched"], 2);
    assert_eq!(payload["summary"]["ambiguous"], 1);
    assert_eq!(payload["summary"]["match_rate"], 0.75);
    assert_eq!(decisions["version"], "canon_entity_link_decisions.v0");
    assert_eq!(decisions["gold_score"]["accuracy"], 1.0);
    assert!(
        decisions["gold_score"]["regressions"]
            .as_array()
            .expect("gold regressions array")
            .is_empty()
    );

    let actual_pairs = decisions["matches"]
        .as_array()
        .expect("matches array")
        .iter()
        .map(|record| {
            (
                record["target_id"].as_str().unwrap().to_string(),
                record["reference_id"].as_str().unwrap().to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let expected_pairs = BTreeMap::from([
        ("WFCM2019-C50|1".to_string(), "223232".to_string()),
        ("WFCM2019-C50|2".to_string(), "223233".to_string()),
        ("WFCM2019-C50|3".to_string(), "223234".to_string()),
        ("WFCM2019-C50|4".to_string(), "223235".to_string()),
        ("WFCM2019-C50|5".to_string(), "223236".to_string()),
        ("WFCM2019-C50|6".to_string(), "223237".to_string()),
        ("WFCM2019-C50|7".to_string(), "223238".to_string()),
        ("WFCM2019-C50|8".to_string(), "223239".to_string()),
        ("WFCM2019-C50|9".to_string(), "223240".to_string()),
    ]);
    assert_eq!(
        actual_pairs, expected_pairs,
        "matched target/reference pairs"
    );

    let unmatched = decisions["unmatched"]
        .as_array()
        .expect("unmatched array")
        .iter()
        .map(|record| record["target_id"].as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        unmatched,
        BTreeSet::from([
            "WFCM2019-C50|404".to_string(),
            "WFCM2019-C50|NULLSERV".to_string(),
        ])
    );

    let ambiguous = decisions["ambiguous"].as_array().expect("ambiguous array");
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0]["target_id"], "WFCM2019-C50|AMB");
    let ambiguous_candidates = ambiguous[0]["candidates"]
        .as_array()
        .expect("ambiguous candidates")
        .iter()
        .map(|candidate| candidate["reference_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ambiguous_candidates, vec!["223240", "223241"]);
}

#[test]
fn unchanged_input_decisions_match_golden_without_mutation() {
    let temp_dir = tempdir().unwrap();
    let registry = temp_dir.path().join("registry");
    copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);
    let (reference, target) = unchanged_entity_link_parity_tapes();
    let input_before = file_digests(&[reference, target]);
    let registry_before = registry_json_digests(&registry);

    let public_a = run_entity_link_json(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        &registry,
        &temp_dir.path().join("public-a"),
        1,
        &["--gold", LOAN_MATCH_GOLD],
    );
    let public_b = run_entity_link_json(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        &registry,
        &temp_dir.path().join("public-b"),
        1,
        &["--gold", LOAN_MATCH_GOLD],
    );

    assert_eq!(
        file_digests(&[reference, target]),
        input_before,
        "read-only parity runs must not mutate checked-in inputs"
    );
    assert_eq!(
        registry_json_digests(&registry),
        registry_before,
        "read-only parity runs must not mutate registry JSON"
    );
    assert_eq!(
        public_a["decision_artifact"], public_b["decision_artifact"],
        "public decision artifact is deterministic even though wrapper work dirs differ"
    );

    let public_decisions = &public_a["decision_artifact"];
    let expected: Value = read_json(UNCHANGED_DECISION_GOLDEN);
    assert_eq!(
        golden_decision_projection(public_decisions),
        expected["projection"],
        "unchanged-input decision projection"
    );

    assert!(
        !public_decisions
            .as_object()
            .expect("decision object")
            .contains_key("write_back"),
        "read-only decision artifact must not include write_back"
    );
    assert_eq!(
        public_a["version"], "canon_entity_link.v0",
        "public wrapper remains the native entity-link artifact"
    );
    assert!(public_a.get("materialized_rows_path").is_some());
    assert!(public_a.get("shared_run_artifact").is_some());
}

#[test]
fn unchanged_input_manifest_and_source_mapping_contract_are_executable() {
    let manifest = read_json(UNCHANGED_MANIFEST);
    let source_mapping = read_json(UNCHANGED_SOURCE_MAPPING);

    assert_eq!(manifest["source_mapping_runtime_use"], json!(false));
    assert_eq!(
        manifest["runtime_inputs"]["reference"]["path"],
        UNCHANGED_REFERENCE_TAPE
    );
    assert_eq!(
        manifest["runtime_inputs"]["reference"]["blake3"],
        blake3_file(Path::new(UNCHANGED_REFERENCE_TAPE))
    );
    assert_eq!(
        manifest["runtime_inputs"]["target"]["path"],
        UNCHANGED_TARGET_TAPE
    );
    assert_eq!(
        manifest["runtime_inputs"]["target"]["blake3"],
        blake3_file(Path::new(UNCHANGED_TARGET_TAPE))
    );
    assert_eq!(
        manifest["source_mapping_artifact"]["path"],
        UNCHANGED_SOURCE_MAPPING
    );
    assert_eq!(
        manifest["source_mapping_artifact"]["blake3"],
        blake3_file(Path::new(UNCHANGED_SOURCE_MAPPING))
    );

    assert_eq!(source_mapping["version"], "canon.source.mapping.v1");
    assert_eq!(
        source_mapping["x-canon-parity-projection"]["target"]["source_row_id"],
        "deal|loan_number"
    );
    assert_eq!(
        source_mapping["x-canon-parity-projection"]["target"]["raw_tenant_name"],
        "servicer_name or UNKNOWN when blank"
    );
    let target_profile = source_mapping["profiles"]
        .as_array()
        .expect("profiles array")
        .iter()
        .find(|profile| profile["profile_id"] == "tests.resolve.target_loans_to_entity_link")
        .expect("target profile");
    assert_eq!(target_profile["object_id_path"], "deal|loan_number");
    assert_eq!(target_profile["locator_path"], "deal|loan_number");
    let source_row_anchor = target_profile["observations"][0]["anchor_mappings"]
        .as_array()
        .expect("anchor mappings")
        .iter()
        .find(|anchor| anchor["namespace"] == "source_row_id")
        .expect("source_row_id anchor");
    assert_eq!(source_row_anchor["path"], "deal|loan_number");
    assert_eq!(
        manifest["refusal_contract"]["max_candidates_zero"]["internal_code"],
        "too_many_candidates"
    );
    assert_eq!(
        manifest["refusal_contract"]["max_candidates_zero"]["public_code"],
        "E_TOO_MANY_CANDIDATES"
    );
}

#[test]
fn unchanged_input_refusal_code_matches_manifest_public_side_without_mutation() {
    let temp_dir = tempdir().unwrap();
    let registry = temp_dir.path().join("registry");
    copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);
    let (reference, target) = unchanged_entity_link_parity_tapes();
    let input_before = file_digests(&[reference, target]);
    let registry_before = registry_json_digests(&registry);
    let manifest = read_json(UNCHANGED_MANIFEST);
    let refusal_contract = &manifest["refusal_contract"]["max_candidates_zero"];

    let refusal_work_dir = temp_dir.path().join("refusal-work");
    let public = run_entity_link_refusal(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        &registry,
        &refusal_work_dir,
        &["--gold", LOAN_MATCH_GOLD, "--max-candidates", "0"],
    );
    assert_eq!(public["outcome"], "REFUSAL");
    assert_eq!(public["refusal"]["code"], refusal_contract["public_code"]);
    assert_eq!(public["refusal"]["detail"]["max_candidates"], 0);
    assert!(
        public["refusal"]["detail"]["target_id"].is_string(),
        "public refusal should retain concrete target context"
    );
    assert!(
        !refusal_work_dir.exists(),
        "public resolve-decision refusal must not create an entity link work-dir"
    );

    assert_eq!(
        file_digests(&[reference, target]),
        input_before,
        "refusal parity must not mutate checked-in inputs"
    );
    assert_eq!(
        registry_json_digests(&registry),
        registry_before,
        "refusal parity must not mutate registry JSON"
    );
}

#[test]
fn conflict_warnings_are_reported_for_one_to_many_matches() {
    let temp_dir = tempdir().unwrap();
    let reference = temp_dir.path().join("reference.csv");
    let target = temp_dir.path().join("target.csv");
    let strategy = temp_dir.path().join("strategy.yaml");
    let registry = temp_dir.path().join("registry");
    fs::create_dir_all(&registry).unwrap();

    fs::write(
        registry.join("registry.json"),
        r#"{
  "id": "resolve-conflict",
  "version": "0.1.0",
  "description": "empty resolve conflict test registry",
  "updated": "2026-05-06",
  "entry_count": 0
}
"#,
    )
    .unwrap();
    fs::write(
        &reference,
        "loan_id,address,source_row_id,deal_id,property_id,raw_tenant_name\nR-1,100 Main St,R-1,D,1,Reference Name\n",
    )
    .unwrap();
    fs::write(
        &target,
        "deal,loan_number,address,source_row_id,deal_id,property_id,raw_tenant_name,loan_id\nD,1,100 Main St,D|1,D,1,Target Name,1\nD,2,100 Main St,D|2,D,2,Target Name,2\n",
    )
    .unwrap();
    fs::write(
        &strategy,
        r#"strategy_id: conflict-test.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
assertions:
  - field_ref: address
    field_tgt: address
    op: exact
    weight: 1.0
    required: true
match_threshold: 1.0
ambiguity_gap: 0.10
"#,
    )
    .unwrap();

    let work_dir = temp_dir.path().join("work");
    let payload =
        run_entity_link_json(&reference, &target, &strategy, &registry, &work_dir, 0, &[]);
    let decisions = &payload["decision_artifact"];

    assert_eq!(decisions["summary"]["matched"], 2);
    let warnings = decisions["conflict_warnings"]
        .as_array()
        .expect("conflict warnings");
    assert_eq!(warnings.len(), 1);
    let warning = warnings[0].as_str().unwrap();
    assert!(warning.contains("one_to_many_conflict"), "{warning}");
    assert!(warning.contains("R-1"), "{warning}");
    assert!(warning.contains("D|1"), "{warning}");
    assert!(warning.contains("D|2"), "{warning}");
}

#[test]
fn writeback_feedback_loop_makes_structural_matches_exactly_lookupable() {
    let temp_dir = tempdir().unwrap();
    let registry = temp_dir.path().join("registry");
    copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);
    let (reference, target) = unchanged_entity_link_parity_tapes();
    let input_before = file_digests(&[reference, target]);
    let payload = run_entity_link_json(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        &registry,
        &temp_dir.path().join("writeback-work"),
        1,
        &["--gold", LOAN_MATCH_GOLD, "--write-back"],
    );
    let decisions = &payload["decision_artifact"];

    assert_eq!(
        file_digests(&[reference, target]),
        input_before,
        "write-back must not mutate input bytes"
    );
    assert_eq!(decisions["write_back"]["written"], true);
    assert_eq!(decisions["write_back"]["entry_count"], 18);
    let mapping_file = decisions["write_back"]["mapping_file"]
        .as_str()
        .expect("mapping file");
    assert!(registry.join(mapping_file).exists());

    let lookup_input = temp_dir.path().join("lookup.jsonl");
    fs::write(
        &lookup_input,
        "{\"id\":\"WFCM2019-C50|1\"}\n{\"id\":\"223232\"}\n",
    )
    .unwrap();
    let assert = canon_std_command_in_manifest()
        .args([
            lookup_input.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--column",
            "id",
            "--explicit",
            "--no-witness",
        ])
        .assert()
        .success();
    let lookup: Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    let mappings = lookup["mappings"].as_array().expect("lookup mappings");
    assert_eq!(lookup["summary"]["resolved"], 2);
    assert!(mappings.iter().any(|mapping| {
        mapping["input"] == "u8:WFCM2019-C50|1" && mapping["canonical_id"] == "u8:223232"
    }));
    assert!(
        mappings.iter().any(
            |mapping| mapping["input"] == "u8:223232" && mapping["canonical_id"] == "u8:223232"
        )
    );
}

fn run_full_entity_link_json(work_dir: &Path, exit_code: i32, extra_args: &[&str]) -> Value {
    let (reference, target) = unchanged_entity_link_parity_tapes();
    run_entity_link_json(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        Path::new("tests/fixtures/registries/resolve-servicers"),
        &work_dir.join("link-work"),
        exit_code,
        &["--gold", LOAN_MATCH_GOLD]
            .into_iter()
            .chain(extra_args.iter().copied())
            .collect::<Vec<_>>(),
    )
}

fn run_entity_link_json(
    reference: &Path,
    target: &Path,
    strategy: &Path,
    registry: &Path,
    work_dir: &Path,
    exit_code: i32,
    extra_args: &[&str],
) -> Value {
    let mut args = vec![
        "entity",
        "link",
        reference.to_str().unwrap(),
        target.to_str().unwrap(),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--work-dir",
        work_dir.to_str().unwrap(),
        "--emit",
        "json",
        "--no-witness",
    ];
    args.extend_from_slice(extra_args);
    let assert = canon_std_command_in_manifest()
        .args(args)
        .assert()
        .code(exit_code);
    serde_json::from_slice(&assert.get_output().stdout).unwrap()
}

fn run_entity_link_refusal(
    reference: &Path,
    target: &Path,
    strategy: &Path,
    registry: &Path,
    work_dir: &Path,
    extra_args: &[&str],
) -> Value {
    let mut args = vec![
        "entity",
        "link",
        reference.to_str().unwrap(),
        target.to_str().unwrap(),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--work-dir",
        work_dir.to_str().unwrap(),
        "--emit",
        "json",
        "--no-witness",
    ];
    args.extend_from_slice(extra_args);
    let assert = canon_std_command_in_manifest().args(args).assert().code(2);
    let output = assert.get_output();
    let bytes = if output.stdout.is_empty() {
        output.stderr.as_slice()
    } else {
        output.stdout.as_slice()
    };
    serde_json::from_slice(bytes).unwrap()
}

fn unchanged_entity_link_parity_tapes() -> (&'static Path, &'static Path) {
    (
        Path::new(UNCHANGED_REFERENCE_TAPE),
        Path::new(UNCHANGED_TARGET_TAPE),
    )
}

fn golden_decision_projection(decisions: &Value) -> Value {
    json!({
        "strategy": decisions["strategy"],
        "registry": {
            "id": decisions["registry"]["id"],
            "version": decisions["registry"]["version"]
        },
        "reference": {
            "rows_path": decisions["reference_tape"]["path"],
            "row_count": decisions["reference_tape"]["record_count"]
        },
        "target": {
            "rows_path": decisions["target_tape"]["path"],
            "row_count": decisions["target_tape"]["record_count"]
        },
        "summary": decisions["summary"],
        "matches": compact_matches(decisions),
        "unmatched": compact_unmatched(decisions),
        "ambiguous": compact_ambiguous(decisions),
        "gold_score": decisions["gold_score"],
        "read_only": {
            "write_back_present": decisions
                .as_object()
                .expect("decision object")
                .contains_key("write_back")
        }
    })
}

fn compact_matches(decisions: &Value) -> Value {
    Value::Array(
        decisions["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|record| {
                json!({
                    "target_id": record["target_id"],
                    "reference_id": record["reference_id"],
                    "canonical_id": record["canonical_id"],
                    "score": record["score"]
                })
            })
            .collect(),
    )
}

fn compact_unmatched(decisions: &Value) -> Value {
    Value::Array(
        decisions["unmatched"]
            .as_array()
            .expect("unmatched")
            .iter()
            .map(|record| {
                json!({
                    "target_id": record["target_id"],
                    "reason": record["reason"]
                })
            })
            .collect(),
    )
}

fn compact_ambiguous(decisions: &Value) -> Value {
    Value::Array(
        decisions["ambiguous"]
            .as_array()
            .expect("ambiguous")
            .iter()
            .map(|record| {
                let candidate_reference_ids = record["candidates"]
                    .as_array()
                    .expect("candidate array")
                    .iter()
                    .map(|candidate| candidate["reference_id"].clone())
                    .collect::<Vec<_>>();
                json!({
                    "target_id": record["target_id"],
                    "reason": record["reason"],
                    "candidate_reference_ids": candidate_reference_ids
                })
            })
            .collect(),
    )
}

fn file_digests(paths: &[&Path]) -> BTreeMap<String, String> {
    paths
        .iter()
        .map(|path| (path.display().to_string(), blake3_file(path)))
        .collect()
}

fn registry_json_digests(registry: &Path) -> BTreeMap<String, String> {
    fs::read_dir(registry)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().to_string(),
                blake3_file(&path),
            )
        })
        .collect()
}

fn blake3_file(path: &Path) -> String {
    format!("blake3:{}", blake3::hash(&fs::read(path).unwrap()).to_hex())
}

fn read_json(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
