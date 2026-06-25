use canon::{
    Refusal, RefusalCode,
    entity::{
        error::EntityRefusalKind,
        profiles::cmbs::{
            CmbsTenantIdAllocationRequest, CmbsTenantIdAllocator, CmbsTenantReservedId,
        },
        surface_id::{SurfaceIdMaterial, derive_surface_ids},
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug, Deserialize)]
struct RefusalMatrix {
    schema_version: String,
    required_codes: Vec<String>,
    cases: Vec<RefusalCase>,
    no_mutation: NoMutationContract,
}

#[derive(Debug, Deserialize)]
struct RefusalCase {
    id: String,
    code: String,
    trigger: String,
    fixture: String,
    expected_stream: String,
    forbidden_outputs: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NoMutationContract {
    sentinel_registry: String,
    sentinel_work_dir: String,
    forbidden_witness_suffix: String,
}

#[derive(Debug, Deserialize)]
struct PatchConflictFixture {
    existing_label: String,
    requested_label: String,
    normalized_display_label: String,
    registry_snapshot_hash: String,
    alias_patch_hash: String,
    existing_review_decision_id: String,
    requested_review_decision_id: String,
}

#[test]
fn prepare_refusal_matrix_fixture_declares_required_codes_and_mutation_guards() {
    let matrix = refusal_matrix();
    assert_eq!(
        matrix.schema_version,
        "canon.entity.prepare_refusal_matrix.v0"
    );
    assert_eq!(
        matrix.required_codes,
        [
            "E_ENTITY_PROFILE",
            "E_ENTITY_INPUT_CONTRACT",
            "E_ENTITY_PATCH_CONFLICT",
            "E_ENTITY_SURFACE_ID_COLLISION",
            "E_ENTITY_ARTIFACT_CONTRACT",
        ]
    );

    let mut seen_codes = BTreeSet::new();
    for case in &matrix.cases {
        assert!(
            seen_codes.insert(case.code.as_str()),
            "duplicate {}",
            case.code
        );
        assert_fixture_exists(&case.fixture);
        assert!(
            case.forbidden_outputs
                .iter()
                .any(|path| path == "prepare/surfaces.jsonl"),
            "{} must guard partial surfaces output",
            case.id
        );
        assert!(
            case.forbidden_outputs
                .iter()
                .any(|path| path == "prepare/prepare.json"),
            "{} must guard partial prepare artifact",
            case.id
        );
    }
    assert_eq!(
        seen_codes,
        matrix
            .required_codes
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
    );
    assert!(fixture_path(&matrix.no_mutation.sentinel_registry).is_dir());
    assert!(fixture_path(&matrix.no_mutation.sentinel_work_dir).is_dir());
    assert!(
        matrix
            .no_mutation
            .forbidden_witness_suffix
            .ends_with("witness.jsonl")
    );
}

#[test]
fn prepare_refusal_matrix_cli_prepare_refusals_preserve_registry_and_workdir() {
    let matrix = refusal_matrix();
    for case in matrix
        .cases
        .iter()
        .filter(|case| case.trigger == "canon_entity_prepare_cli")
    {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = copy_fixture_tree(
            &fixture_path(&matrix.no_mutation.sentinel_registry),
            &temp.path().join("registry"),
        );
        let work_dir = copy_fixture_tree(
            &fixture_path(&matrix.no_mutation.sentinel_work_dir),
            &temp.path().join("work"),
        );
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("home dir");
        let before_registry = TreeSnapshot::capture(&registry);
        let before_work = TreeSnapshot::capture(&work_dir);

        let profile = if case.code == "E_ENTITY_PROFILE" {
            "unknown_profile"
        } else {
            "cmbs_tenant_label"
        };
        let output = canon_command()
            .arg("entity")
            .arg("prepare")
            .arg(fixture_path(&case.fixture))
            .arg("--profile")
            .arg(profile)
            .arg("--registry")
            .arg(&registry)
            .arg("--work-dir")
            .arg(&work_dir)
            .env("HOME", &home)
            .output()
            .unwrap_or_else(|error| panic!("{} command runs: {error}", case.id));

        assert_refusal_output(case, &output);
        before_registry.assert_unchanged(&TreeSnapshot::capture(&registry));
        before_work.assert_unchanged(&TreeSnapshot::capture(&work_dir));
        assert_forbidden_outputs_absent(&work_dir, case);
        assert!(
            !home
                .join(&matrix.no_mutation.forbidden_witness_suffix)
                .exists(),
            "{} must not mutate witness ledger on refusal",
            case.id
        );
    }
}

#[test]
fn prepare_refusal_matrix_synthetic_refusals_are_actionable_and_no_mutation() {
    let matrix = refusal_matrix();
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = copy_fixture_tree(
        &fixture_path(&matrix.no_mutation.sentinel_registry),
        &temp.path().join("registry"),
    );
    let work_dir = copy_fixture_tree(
        &fixture_path(&matrix.no_mutation.sentinel_work_dir),
        &temp.path().join("work"),
    );
    let before_registry = TreeSnapshot::capture(&registry);
    let before_work = TreeSnapshot::capture(&work_dir);

    for case in matrix
        .cases
        .iter()
        .filter(|case| case.trigger != "canon_entity_prepare_cli")
    {
        let refusal = match case.trigger.as_str() {
            "cmbs_tenant_allocator_patch_conflict" => patch_conflict_refusal(case),
            "synthetic_surface_id_collision" => synthetic_surface_collision_refusal(case),
            "surface_id_artifact_contract" => artifact_contract_refusal(case),
            trigger => panic!("unknown trigger {trigger}"),
        };
        assert_refusal_contract(case, refusal);
        assert_forbidden_outputs_absent(&work_dir, case);
    }

    before_registry.assert_unchanged(&TreeSnapshot::capture(&registry));
    before_work.assert_unchanged(&TreeSnapshot::capture(&work_dir));
}

#[test]
fn entity_refusal_no_mutation_forbids_partial_prepare_artifacts_and_cache_dirs() {
    let matrix = refusal_matrix();
    let case = matrix
        .cases
        .iter()
        .find(|case| case.code == "E_ENTITY_INPUT_CONTRACT")
        .expect("input contract case exists");
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = copy_fixture_tree(
        &fixture_path(&matrix.no_mutation.sentinel_registry),
        &temp.path().join("registry"),
    );
    let work_dir = copy_fixture_tree(
        &fixture_path(&matrix.no_mutation.sentinel_work_dir),
        &temp.path().join("work"),
    );
    let before_registry = TreeSnapshot::capture(&registry);
    let before_work = TreeSnapshot::capture(&work_dir);

    let output = canon_command()
        .arg("entity")
        .arg("prepare")
        .arg(fixture_path(&case.fixture))
        .args(["--profile", "cmbs_tenant_label", "--registry"])
        .arg(&registry)
        .arg("--work-dir")
        .arg(&work_dir)
        .output()
        .expect("prepare refusal command runs");

    assert_refusal_output(case, &output);
    before_registry.assert_unchanged(&TreeSnapshot::capture(&registry));
    before_work.assert_unchanged(&TreeSnapshot::capture(&work_dir));
    assert_forbidden_outputs_absent(&work_dir, case);
}

fn refusal_matrix() -> RefusalMatrix {
    let raw = fs::read_to_string(fixture_path(
        "tests/fixtures/entity/prepare/refusals/matrix.json",
    ))
    .expect("refusal matrix fixture opens");
    serde_json::from_str(&raw).expect("refusal matrix fixture parses")
}

fn patch_conflict_refusal(case: &RefusalCase) -> Refusal {
    let fixture: PatchConflictFixture =
        serde_json::from_str(&fs::read_to_string(fixture_path(&case.fixture)).expect("fixture"))
            .expect("patch conflict fixture parses");
    let existing = CmbsTenantIdAllocationRequest::new(
        fixture.existing_label,
        fixture.normalized_display_label.clone(),
        fixture.registry_snapshot_hash.clone(),
        fixture.alias_patch_hash.clone(),
        fixture.existing_review_decision_id,
    );
    let requested = CmbsTenantIdAllocationRequest::new(
        fixture.requested_label,
        fixture.normalized_display_label,
        fixture.registry_snapshot_hash,
        fixture.alias_patch_hash,
        fixture.requested_review_decision_id,
    );
    CmbsTenantIdAllocator::new([CmbsTenantReservedId::new(
        "TNT-SEARS",
        existing.replay_key(),
    )])
    .allocate(&requested)
    .expect_err("patch conflict refuses")
}

fn synthetic_surface_collision_refusal(case: &RefusalCase) -> Refusal {
    let detail: Value =
        serde_json::from_str(&fs::read_to_string(fixture_path(&case.fixture)).expect("fixture"))
            .expect("surface collision fixture parses");
    EntityRefusalKind::SurfaceIdCollision.to_refusal(
        "Prepared surfaces produced the same surface_id",
        detail,
        None,
    )
}

fn artifact_contract_refusal(case: &RefusalCase) -> Refusal {
    let fixture: Value =
        serde_json::from_str(&fs::read_to_string(fixture_path(&case.fixture)).expect("fixture"))
            .expect("artifact contract fixture parses");
    let material = SurfaceIdMaterial::new(
        fixture["profile_id"].as_str().unwrap_or_default(),
        fixture["normalized_view_name"].as_str().unwrap_or_default(),
        fixture["normalized_view_value"]
            .as_str()
            .unwrap_or_default(),
        Vec::<String>::new(),
    );
    derive_surface_ids(&[material]).expect_err("bad surface material refuses")
}

fn assert_refusal_output(case: &RefusalCase, output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "{} exit code: stderr={}",
        case.id,
        String::from_utf8_lossy(&output.stderr)
    );
    match case.expected_stream.as_str() {
        "stdout" => {
            assert!(
                output.stderr.is_empty(),
                "{} stderr must stay empty",
                case.id
            );
            let value: Value =
                serde_json::from_slice(&output.stdout).expect("stdout refusal JSON parses");
            assert_refusal_envelope(&value, &case.code);
        }
        "stderr" => {
            assert!(
                output.stdout.is_empty(),
                "{} stdout must stay empty",
                case.id
            );
            let value: Value =
                serde_json::from_slice(&output.stderr).expect("stderr refusal JSON parses");
            assert_refusal_envelope(&value, &case.code);
        }
        stream => panic!("unknown expected stream {stream}"),
    }
}

fn assert_refusal_contract(case: &RefusalCase, refusal: Refusal) {
    assert_eq!(refusal.code.as_str(), case.code);
    assert!(!refusal.message.trim().is_empty());
    assert!(refusal.detail.is_object());
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|next| !next.trim().is_empty())
    );
    let output = refusal.to_canon_output();
    let value = serde_json::to_value(output).expect("refusal output serializes");
    assert_refusal_envelope(&value, &case.code);
}

fn assert_forbidden_outputs_absent(work_dir: &Path, case: &RefusalCase) {
    for relative in &case.forbidden_outputs {
        assert!(
            !work_dir.join(relative).exists(),
            "{} created forbidden output {}",
            case.id,
            relative
        );
    }
}

fn assert_fixture_exists(relative: &str) {
    assert!(
        fixture_path(relative).exists(),
        "fixture path {relative} must exist"
    );
}

fn canon_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.current_dir(manifest_dir());
    command
}

fn copy_fixture_tree(source: &Path, dest: &Path) -> PathBuf {
    copy_dir_recursive(source, dest);
    dest.to_path_buf()
}

fn copy_dir_recursive(source: &Path, dest: &Path) {
    fs::create_dir_all(dest).expect("destination dir");
    let mut entries = fs::read_dir(source)
        .unwrap_or_else(|error| panic!("read_dir {}: {error}", source.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let target = dest.join(path.file_name().expect("fixture entry name"));
        if path.is_dir() {
            copy_dir_recursive(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap_or_else(|error| {
                panic!("copy {} to {}: {error}", path.display(), target.display())
            });
        }
    }
}

fn fixture_path(relative: &str) -> PathBuf {
    manifest_dir().join(relative)
}

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

trait RefusalCodeString {
    fn as_str(&self) -> &'static str;
}

impl RefusalCodeString for RefusalCode {
    fn as_str(&self) -> &'static str {
        match self {
            RefusalCode::EEntityProfile => "E_ENTITY_PROFILE",
            RefusalCode::EEntityInputContract => "E_ENTITY_INPUT_CONTRACT",
            RefusalCode::EEntityPatchConflict => "E_ENTITY_PATCH_CONFLICT",
            RefusalCode::EEntitySurfaceIdCollision => "E_ENTITY_SURFACE_ID_COLLISION",
            RefusalCode::EEntityArtifactContract => "E_ENTITY_ARTIFACT_CONTRACT",
            other => panic!("unexpected refusal code {other:?}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeSnapshot {
    files: std::collections::BTreeMap<PathBuf, String>,
}

impl TreeSnapshot {
    fn capture(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let mut files = std::collections::BTreeMap::new();
        capture_tree(root, root, &mut files);
        Self { files }
    }

    fn assert_unchanged(&self, after: &Self) {
        assert_eq!(self.files, after.files, "tree changed after refusal");
    }
}

fn capture_tree(
    root: &Path,
    current: &Path,
    files: &mut std::collections::BTreeMap<PathBuf, String>,
) {
    let mut entries = fs::read_dir(current)
        .unwrap_or_else(|error| panic!("read_dir {}: {error}", current.display()))
        .map(|entry| entry.expect("directory entry can be read").path())
        .collect::<Vec<_>>();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            capture_tree(root, &path, files);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("captured file is under root")
                .to_path_buf();
            files.insert(relative, blake3_file(&path));
        }
    }
}

fn blake3_file(path: &Path) -> String {
    let bytes = fs::read(path).expect("hash input file can be read");
    format!("blake3:{}", blake3::hash(&bytes).to_hex())
}

fn assert_refusal_envelope(value: &Value, expected_code: &str) {
    assert_eq!(value["version"], "canon.v0");
    assert_eq!(value["outcome"], "REFUSAL");
    assert_eq!(value["refusal"]["code"], expected_code);
    assert!(
        value["refusal"]["message"]
            .as_str()
            .is_some_and(|message| !message.trim().is_empty()),
        "refusal message must be present"
    );
    assert!(
        value["refusal"]["detail"].is_object(),
        "refusal detail must be an object"
    );
    assert!(
        value["refusal"]["next_command"]
            .as_str()
            .is_some_and(|next| !next.trim().is_empty()),
        "refusal next_command must be present"
    );
}
