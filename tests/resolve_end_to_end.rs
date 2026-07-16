use assert_cmd::prelude::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
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

#[test]
fn legacy_cmbs_loan_strategy_refuses_native_tenant_profile_without_writes() {
    let temp_dir = tempdir().unwrap();
    let registry = temp_dir.path().join("registry");
    copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);
    let (reference, target) = unchanged_entity_link_parity_tapes();
    let input_before = file_digests(&[reference, target]);
    let registry_before = registry_json_digests(&registry);
    let work_dir = temp_dir.path().join("link-work");
    let public = run_entity_link_refusal(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        &registry,
        &work_dir,
        &["--gold", LOAN_MATCH_GOLD],
    );

    assert_legacy_strategy_input_contract_refusal(&public, "cmbs-loan-match.v1");
    assert!(
        !work_dir.exists(),
        "cutover refusal must happen before writes"
    );
    assert_eq!(
        file_digests(&[reference, target]),
        input_before,
        "cutover refusal must not mutate checked-in inputs"
    );
    assert_eq!(
        registry_json_digests(&registry),
        registry_before,
        "cutover refusal must not mutate registry JSON"
    );
}

#[test]
fn legacy_decision_projection_is_not_blessed_as_v1_matching_acceptance() {
    let temp_dir = tempdir().unwrap();
    let registry = temp_dir.path().join("registry");
    copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);
    let (reference, target) = unchanged_entity_link_parity_tapes();
    let input_before = file_digests(&[reference, target]);
    let registry_before = registry_json_digests(&registry);

    let public_a = run_entity_link_refusal(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        &registry,
        &temp_dir.path().join("public-a"),
        &["--gold", LOAN_MATCH_GOLD],
    );
    let public_b = run_entity_link_refusal(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        &registry,
        &temp_dir.path().join("public-b"),
        &["--gold", LOAN_MATCH_GOLD],
    );

    assert_legacy_strategy_input_contract_refusal(&public_a, "cmbs-loan-match.v1");
    assert_legacy_strategy_input_contract_refusal(&public_b, "cmbs-loan-match.v1");

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
fn legacy_one_to_many_strategy_refuses_instead_of_synthesizing_v1_conflict_warning() {
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
        run_entity_link_refusal(&reference, &target, &strategy, &registry, &work_dir, &[]);

    assert_legacy_strategy_input_contract_refusal(&payload, "conflict-test.v1");
    assert!(
        !work_dir.exists(),
        "mismatch refusal must happen before writes"
    );
}

#[test]
fn writeback_refuses_before_mutating_registry_or_inputs() {
    let temp_dir = tempdir().unwrap();
    let registry = temp_dir.path().join("registry");
    copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);
    let (reference, target) = unchanged_entity_link_parity_tapes();
    let input_before = file_digests(&[reference, target]);
    let registry_before = registry_json_digests(&registry);
    let work_dir = temp_dir.path().join("writeback-work");
    let payload = run_entity_link_refusal(
        reference,
        target,
        Path::new(CMBS_STRATEGY),
        &registry,
        &work_dir,
        &["--gold", LOAN_MATCH_GOLD, "--write-back"],
    );

    assert_eq!(
        file_digests(&[reference, target]),
        input_before,
        "refused write-back must not mutate input bytes"
    );
    assert!(
        !work_dir.exists(),
        "write-back refusal should happen before work-dir writes"
    );
    assert_eq!(
        registry_json_digests(&registry),
        registry_before,
        "refused write-back must not mutate registry JSON"
    );
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(payload["refusal"]["detail"]["flag"], "--write-back");
    assert_eq!(
        payload["refusal"]["detail"]["reason"],
        "transactional_publication_required"
    );
    assert_eq!(payload["refusal"]["detail"]["writes_performed"], false);
    assert_eq!(
        payload["refusal"]["detail"]["registry_write_back_performed"],
        false
    );
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

fn assert_legacy_strategy_input_contract_refusal(public: &Value, strategy_id: &str) {
    assert_eq!(public["outcome"], "REFUSAL");
    assert_eq!(public["refusal"]["code"], "E_ENTITY_INPUT_CONTRACT");
    let next_command = public["refusal"]["next_command"]
        .as_str()
        .expect("actionable next command");
    assert!(
        next_command.contains("entity_type 'loan'"),
        "{next_command}"
    );
    assert!(next_command.contains("cmbs_tenant_label"), "{next_command}");
    let detail = &public["refusal"]["detail"];
    assert_eq!(detail["stage"], "link");
    assert_eq!(detail["field"], "profile.entity_type");
    assert_eq!(detail["profile_source"], "cmbs_tenant_label");
    assert_eq!(detail["expected"]["strategy_entity_type"], "loan");
    assert_eq!(detail["expected"]["strategy_id"], strategy_id);
    assert_eq!(detail["expected"]["strategy_version"], "0.1.0");
    assert!(
        detail["expected"]["strategy_content_hash"].is_string(),
        "strategy hash must be present"
    );
    assert_eq!(detail["actual"]["profile_entity_type"], "tenant_label");
    assert_eq!(detail["actual"]["profile_id"], "cmbs_tenant_label");
    assert_eq!(detail["actual"]["profile_version"], "0.1.0");
    assert!(
        detail["actual"]["profile_content_hash"].is_string(),
        "profile hash must be present"
    );
    assert_eq!(detail["writes_performed"], false);
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
