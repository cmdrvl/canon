use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_PREPARE_VERSION_V1,
        prepare::{
            PrepareRunRequest, PreparedExactLookupStatus, PreparedSurfaceRecord, run_prepare,
            run_prepare_v1,
        },
        schema::{validate_artifact_v1_core_contract, validate_entity_v1_self_hash},
        surface_id::{SurfaceIdMaterial, derive_surface_ids},
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

const PREPARE_FIXTURES: &str = "tests/fixtures/entity/prepare";
const CMBS_REGISTRY: &str = "tests/fixtures/registries/empty";
const REGAB_ROWS: &str =
    "tests/fixtures/entity/regab/sec10d_baseline_public/org_mentions_selected.csv";
const REGAB_REGISTRY: &str =
    "tests/fixtures/entity/regab/sec10d_baseline_public/registry_snapshot/firms";

#[test]
#[allow(non_snake_case)]
fn EN_P001_fixture_duplicates_collapse_to_expected_surface() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact = run_prepare(PrepareRunRequest {
        rows: Path::new(PREPARE_FIXTURES)
            .join("en_p001_rows.csv")
            .as_path(),
        profile: "cmbs_tenant_label",
        registry: Path::new(CMBS_REGISTRY),
        work_dir: temp.path(),
    })
    .expect("prepare run");
    let expected = read_json("en_p001_prepare.json");
    let surfaces = read_surfaces(temp.path().join(&artifact.surfaces_path));

    assert_summary(&artifact.summary, &expected["summary"]);
    assert_eq!(
        compact_surface(&surfaces[0]),
        expected["surfaces"][0],
        "EN-P001 compact surface projection"
    );
    assert_eq!(surfaces[0].provenance_samples.len(), 6);
}

#[test]
#[allow(non_snake_case)]
fn EN_P002_fixture_row_shuffle_keeps_surface_jsonl_stable() {
    let expected = read_json("en_p002_prepare.json");
    let original_dir = tempfile::tempdir().expect("original tempdir");
    let shuffled_dir = tempfile::tempdir().expect("shuffled tempdir");
    let original = run_fixture_prepare("en_p002_rows.csv", original_dir.path());
    let shuffled = run_fixture_prepare("en_p002_rows_shuffled.csv", shuffled_dir.path());

    assert_summary(&original.summary, &expected["summary"]);
    assert_summary(&shuffled.summary, &expected["summary"]);

    let original_surfaces =
        fs::read_to_string(original_dir.path().join(&original.surfaces_path)).expect("surfaces");
    let shuffled_surfaces =
        fs::read_to_string(shuffled_dir.path().join(&shuffled.surfaces_path)).expect("surfaces");
    assert_eq!(original_surfaces, shuffled_surfaces);

    let mut surface_keys = read_surfaces(original_dir.path().join(&original.surfaces_path))
        .into_iter()
        .map(|surface| surface.surface_key)
        .collect::<Vec<_>>();
    surface_keys.sort();
    assert_eq!(json!(surface_keys), expected["surface_keys"]);
}

#[test]
#[allow(non_snake_case)]
fn EN_P003_fixture_malformed_alias_json_refuses_without_prepare_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let sentinel = temp.path().join("KEEP.txt");
    fs::write(&sentinel, "sentinel\n").expect("sentinel");
    let before_sentinel = fs::read(&sentinel).expect("sentinel bytes");
    let expected = read_json("en_p003_refusal.json");
    let refusal = run_prepare(PrepareRunRequest {
        rows: Path::new(PREPARE_FIXTURES)
            .join("en_p003_bad_alias.jsonl")
            .as_path(),
        profile: "regab_firm_identity",
        registry: Path::new(REGAB_REGISTRY),
        work_dir: temp.path(),
    })
    .expect_err("bad side field refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityInputContract);
    assert_eq!(expected["code"], "E_ENTITY_INPUT_CONTRACT");
    assert_eq!(
        fs::read(&sentinel).expect("sentinel after refusal"),
        before_sentinel
    );
    assert!(!temp.path().join("prepare").join("prepare.json").exists());
    assert!(!temp.path().join("prepare").join("surfaces.jsonl").exists());
}

#[test]
fn prepare_refusals_no_mutation() {
    EN_P003_fixture_malformed_alias_json_refuses_without_prepare_artifacts();
}

#[test]
#[allow(non_snake_case)]
fn EN_P004_fixture_pins_synthetic_surface_collision_material() {
    let expected = read_json("en_p004_refusal.json");
    let materials = fs::read_to_string(Path::new(PREPARE_FIXTURES).join("en_p004_collision.jsonl"))
        .expect("collision fixture")
        .lines()
        .map(|line| serde_json::from_str::<SurfaceIdMaterial>(line).expect("surface material"))
        .collect::<Vec<_>>();

    assert_eq!(materials.len(), 2);
    assert_ne!(materials[0], materials[1]);
    assert_eq!(expected["code"], "E_ENTITY_SURFACE_ID_COLLISION");
    assert_eq!(expected["collision_policy"], "refuse_without_silent_rekey");
    derive_surface_ids(&materials).expect("real hasher should not collide for fixture material");
}

#[test]
#[allow(non_snake_case)]
fn EN_P005_fixture_existing_registry_aliases_are_pre_resolved() {
    let temp = tempfile::tempdir().expect("tempdir");
    let expected = read_json("en_p005_prepare.json");
    let artifact = run_prepare(PrepareRunRequest {
        rows: Path::new(REGAB_ROWS),
        profile: "regab_firm_identity",
        registry: Path::new(REGAB_REGISTRY),
        work_dir: temp.path(),
    })
    .expect("prepare run");
    let surfaces = read_surfaces(temp.path().join(&artifact.surfaces_path));
    let computershare = surfaces
        .iter()
        .find(|surface| {
            surface
                .raw_variants
                .iter()
                .any(|raw| raw == "Computershare")
        })
        .expect("Computershare surface");

    assert!(
        artifact.summary["exact_resolved_surfaces"]
            >= expected["summary_at_least"]["exact_resolved_surfaces"]
                .as_u64()
                .expect("minimum")
    );
    assert_eq!(
        computershare.exact_lookup.status,
        PreparedExactLookupStatus::Resolved
    );
    assert_eq!(
        computershare.exact_lookup.canonical_id.as_deref(),
        expected["resolved_sample"]["canonical_id"].as_str()
    );
    assert_eq!(
        computershare.exact_lookup.rule_id.as_deref(),
        expected["resolved_sample"]["rule_id"].as_str()
    );
    assert_eq!(
        computershare
            .exact_lookup
            .registry_snapshot
            .as_ref()
            .expect("registry snapshot")
            .lookup_snapshot_hash,
        artifact.registry_snapshot.lookup_snapshot_hash
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_P006_prepare_v1_artifact_has_schema_hash_self_hash_and_firewall_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact = run_prepare_v1(PrepareRunRequest {
        rows: Path::new(PREPARE_FIXTURES)
            .join("en_p001_rows.csv")
            .as_path(),
        profile: "cmbs_tenant_label",
        registry: Path::new(CMBS_REGISTRY),
        work_dir: temp.path(),
    })
    .expect("prepare v1 run");
    let artifact_bytes =
        fs::read(temp.path().join("prepare").join("prepare.json")).expect("prepare artifact");

    assert_eq!(artifact["version"], CANON_ENTITY_PREPARE_VERSION_V1);
    assert_eq!(
        validate_artifact_v1_core_contract(&artifact)
            .expect("prepare v1 core contract")
            .artifact_version,
        CANON_ENTITY_PREPARE_VERSION_V1
    );
    assert_eq!(
        validate_entity_v1_self_hash(&artifact).expect("prepare v1 self hash"),
        artifact["artifact_content_hash"].as_str().expect("hash")
    );
    assert_eq!(
        artifact["metadata"]["schema"]["key"],
        CANON_ENTITY_PREPARE_VERSION_V1
    );
    assert_eq!(artifact["metadata"]["workdir"]["stage_dir"], "prepare");
    assert_eq!(
        artifact["metadata"]["workdir"]["artifact_relpath"],
        "prepare/prepare.json"
    );
    assert_eq!(
        artifact["metadata"]["workdir"]["payload_relpath"],
        "prepare/surfaces.jsonl"
    );
    assert_eq!(
        artifact["metadata"]["patch_namespace"],
        artifact["metadata"]["profile"]["patch_namespaces"]["aliases"]
    );
    assert!(
        artifact["metadata"]["upstream_artifacts"]
            .as_array()
            .expect("upstreams")
            .is_empty()
    );
    assert!(
        !std::str::from_utf8(&artifact_bytes)
            .expect("utf8 artifact")
            .contains("canon_entity_prepare.v0"),
        "prepare v1 artifact must not serialize a v0 backing version"
    );
    assert!(temp.path().join("prepare").join("surfaces.jsonl").exists());
}

#[test]
fn entity_prepare_cli_emits_true_v1_artifact_with_hash_contract() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "prepare",
            Path::new(PREPARE_FIXTURES)
                .join("en_p001_rows.csv")
                .to_str()
                .expect("rows path"),
            "--profile",
            "cmbs_tenant_label",
            "--registry",
            CMBS_REGISTRY,
            "--work-dir",
            temp.path().to_str().expect("work dir path"),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let artifact: Value = serde_json::from_slice(&output).expect("prepare cli artifact");

    assert_eq!(artifact["version"], CANON_ENTITY_PREPARE_VERSION_V1);
    assert_eq!(
        validate_artifact_v1_core_contract(&artifact)
            .expect("prepare cli core contract")
            .artifact_version,
        CANON_ENTITY_PREPARE_VERSION_V1
    );
    assert_eq!(
        validate_entity_v1_self_hash(&artifact).expect("prepare cli self hash"),
        artifact["artifact_content_hash"].as_str().expect("hash")
    );
    assert_eq!(
        artifact["metadata"]["schema"]["key"],
        CANON_ENTITY_PREPARE_VERSION_V1
    );
    assert!(
        !std::str::from_utf8(&output)
            .expect("utf8 artifact")
            .contains("canon_entity_prepare.v0")
    );
    assert_eq!(
        serde_json::from_slice::<Value>(
            &fs::read(temp.path().join("prepare").join("prepare.json")).expect("persisted prepare")
        )
        .expect("persisted prepare json"),
        artifact
    );
}

fn run_fixture_prepare(
    file_name: &str,
    work_dir: &Path,
) -> canon::entity::prepare::PrepareRunArtifact {
    run_prepare(PrepareRunRequest {
        rows: Path::new(PREPARE_FIXTURES).join(file_name).as_path(),
        profile: "cmbs_tenant_label",
        registry: Path::new(CMBS_REGISTRY),
        work_dir,
    })
    .expect("prepare run")
}

fn read_json(file_name: &str) -> Value {
    let path = Path::new(PREPARE_FIXTURES).join(file_name);
    serde_json::from_str(&fs::read_to_string(path).expect("expected json"))
        .expect("expected json parses")
}

fn read_surfaces(path: impl AsRef<Path>) -> Vec<PreparedSurfaceRecord> {
    fs::read_to_string(path)
        .expect("surfaces jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("surface record"))
        .collect()
}

fn assert_summary(actual: &BTreeMap<String, u64>, expected: &Value) {
    for (key, expected_value) in expected.as_object().expect("summary object") {
        assert_eq!(
            actual[key],
            expected_value.as_u64().expect("summary value"),
            "summary counter {key}"
        );
    }
}

fn compact_surface(surface: &PreparedSurfaceRecord) -> Value {
    json!({
        "surface_key": surface.surface_key,
        "primary_surface": surface.primary_surface,
        "tenant_core": surface.normalized_views["tenant_core"].value,
        "raw_variants": surface.raw_variants,
        "alias_surfaces": surface.alias_surfaces,
        "mention_surfaces": surface.mention_surfaces,
        "row_count": surface.row_count,
        "deal_count": surface.deal_count,
        "exact_lookup": {
            "status": surface.exact_lookup.status,
            "canonical_id": surface.exact_lookup.canonical_id,
        }
    })
}
