#![forbid(unsafe_code)]

#[path = "../src/operator.rs"]
mod operator;

mod distribution {
    pub mod backend {
        include!("../src/distribution/backend.rs");
    }
}

use assert_cmd::Command;
use distribution::backend::{
    FilesystemPublicationBackend, PublicationErrorKind, PublicationRequest, PublishedPackageRef,
};
use operator::{
    COMMAND_SAFETY_DECLARATIONS, CORE_PLATFORM_CLASSES, ConcurrencyClass, MutationClass,
    NetworkClass, PlatformClass, SAFETY_MATRIX_SCHEMA_VERSION, declaration_for,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::UNIX_EPOCH,
};
use tempfile::TempDir;

const WORKER_ENV: &str = "CANON_SAFETY_MATRIX_WORKER";
const ROOT_ENV: &str = "CANON_SAFETY_MATRIX_ROOT";
const CHANNEL_ENV: &str = "CANON_SAFETY_MATRIX_CHANNEL";
const BASE_DIGEST_ENV: &str = "CANON_SAFETY_MATRIX_BASE_DIGEST";
const EXPECTED_DIGEST_ENV: &str = "CANON_SAFETY_MATRIX_EXPECTED_DIGEST";
const WORKER_INDEX_ENV: &str = "CANON_SAFETY_MATRIX_WORKER_INDEX";

#[derive(Debug, Clone, PartialEq, Eq)]
enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MetadataSnapshot {
    kind: EntryKind,
    len: u64,
    readonly: bool,
    modified_nanos: Option<u128>,
    mode: Option<u32>,
    content_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetworkPolicy {
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HarnessRefusal {
    NetworkDeniedBeforeSpawn { command: String },
}

#[test]
fn safety_matrix_declarations_cover_required_classes_and_platforms() {
    assert_eq!(
        SAFETY_MATRIX_SCHEMA_VERSION,
        "canon.operator.safety_matrix.v1"
    );

    let commands = COMMAND_SAFETY_DECLARATIONS
        .iter()
        .map(|declaration| declaration.command)
        .collect::<BTreeSet<_>>();
    for required in [
        "doctor",
        "lookup",
        "package inspect",
        "package verify",
        "registry export",
        "registry build openfigi",
        "strategy audit",
        "project read",
        "unresolved inbox read",
        "publication backend",
    ] {
        assert!(commands.contains(required), "missing safety row {required}");
    }

    let mutations = COMMAND_SAFETY_DECLARATIONS
        .iter()
        .map(|declaration| declaration.mutation)
        .collect::<BTreeSet<_>>();
    for required in [
        MutationClass::ReadOnly,
        MutationClass::OwnedOutput,
        MutationClass::CacheOnly,
        MutationClass::RegistryMutation,
        MutationClass::PublicationTransaction,
        MutationClass::ExternalMaterialization,
    ] {
        assert!(mutations.contains(&required), "missing {required:?}");
    }

    let network = COMMAND_SAFETY_DECLARATIONS
        .iter()
        .map(|declaration| declaration.network)
        .collect::<BTreeSet<_>>();
    assert!(network.contains(&NetworkClass::Offline));
    assert!(network.contains(&NetworkClass::DeniedByDefault));
    assert!(network.contains(&NetworkClass::ExplicitExternalProvider));

    let concurrency = COMMAND_SAFETY_DECLARATIONS
        .iter()
        .map(|declaration| declaration.concurrency)
        .collect::<BTreeSet<_>>();
    for required in [
        ConcurrencyClass::StatelessRead,
        ConcurrencyClass::AtomicOwnedOutput,
        ConcurrencyClass::CacheRaceSafe,
        ConcurrencyClass::ExclusiveRegistryMutation,
        ConcurrencyClass::OptimisticPublicationCas,
        ConcurrencyClass::IsolatedRunner,
    ] {
        assert!(concurrency.contains(&required), "missing {required:?}");
    }

    for required in [
        PlatformClass::PortablePathUtf8,
        PlatformClass::SameFilesystemAtomicReplace,
        PlatformClass::SameFilesystemAtomicNoClobber,
        PlatformClass::UnixPermissionBits,
        PlatformClass::RejectLinks,
        PlatformClass::AdvisoryFileLock,
    ] {
        assert!(CORE_PLATFORM_CLASSES.contains(&required));
        assert!(
            COMMAND_SAFETY_DECLARATIONS
                .iter()
                .any(|declaration| declaration.platforms.contains(&required)),
            "no declaration covers {required:?}"
        );
    }

    for declaration in COMMAND_SAFETY_DECLARATIONS {
        assert!(
            declaration.owned_temp_fixtures_only,
            "{} must keep tests in owned temp fixtures",
            declaration.command
        );
        assert!(
            !declaration.usage.trim().is_empty() && !declaration.notes.trim().is_empty(),
            "{} needs operator-facing usage and safety notes",
            declaration.command
        );
        assert_eq!(
            declaration.read_only,
            declaration.mutation == MutationClass::ReadOnly,
            "{} read_only flag must match mutation class",
            declaration.command
        );
    }
}

#[test]
fn operator_json_side_effects_match_declared_offline_read_only_rows() {
    let operator_json: Value =
        serde_json::from_str(include_str!("../operator.json")).expect("operator.json parses");
    let commands = operator_json["subcommands"]
        .as_array()
        .expect("subcommands array")
        .iter()
        .filter_map(|command| {
            command["name"]
                .as_str()
                .map(|name| (name.to_string(), command))
        })
        .collect::<BTreeMap<_, _>>();

    for declaration in COMMAND_SAFETY_DECLARATIONS {
        let Some(contract_name) = declaration.operator_contract_name else {
            continue;
        };
        let Some(contract) = commands.get(contract_name) else {
            continue;
        };

        if let Some(read_only) = contract["read_only"].as_bool() {
            assert_eq!(
                read_only, declaration.read_only,
                "{} read_only drifted from operator.json",
                declaration.command
            );
        }
        if declaration.network == NetworkClass::Offline && contract["side_effects"].is_object() {
            assert_eq!(
                contract["side_effects"]["uses_network"].as_bool(),
                Some(false),
                "{} must stay offline in operator.json",
                declaration.command
            );
        }
    }

    assert_eq!(
        declaration_for("package verify").unwrap().network,
        NetworkClass::Offline
    );
    assert_eq!(
        declaration_for("publication backend").unwrap().concurrency,
        ConcurrencyClass::OptimisticPublicationCas
    );
}

#[test]
fn offline_harness_refuses_network_capable_cases_before_spawn() {
    let openfigi = declaration_for("registry build openfigi").unwrap();
    let refusal = enforce_network_policy(NetworkPolicy::Deny, openfigi).unwrap_err();
    assert_eq!(
        refusal,
        HarnessRefusal::NetworkDeniedBeforeSpawn {
            command: "registry build openfigi".to_string()
        }
    );

    let mock = declaration_for("registry build mock").unwrap();
    enforce_network_policy(NetworkPolicy::Deny, mock).unwrap();

    let providers = declaration_for("registry providers").unwrap();
    enforce_network_policy(NetworkPolicy::Deny, providers).unwrap();
}

#[test]
fn read_only_commands_preserve_content_mtime_permissions_and_inventory()
-> Result<(), Box<dyn Error>> {
    let fixture = SafetyFixture::new()?;
    let before = snapshot_tree(&fixture.observed_root)?;

    run_offline_canon(&fixture, ["doctor", "health", "--json"])?
        .assert()
        .success();
    run_offline_canon(&fixture, ["doctor", "capabilities", "--json"])?
        .assert()
        .success();
    run_offline_canon(&fixture, ["registry", "providers", "--emit", "json"])?
        .assert()
        .success();
    run_offline_canon(
        &fixture,
        ["registry", "provider-schema", "mock", "--emit", "json"],
    )?
    .assert()
    .success();
    run_offline_canon(
        &fixture,
        [
            OsStr::new("registry"),
            OsStr::new("lint"),
            fixture.registry.as_os_str(),
            OsStr::new("--emit"),
            OsStr::new("json"),
        ],
    )?
    .assert()
    .success();
    run_offline_canon(
        &fixture,
        [
            OsStr::new("strategy"),
            OsStr::new("profile"),
            fixture.input_csv.as_os_str(),
            OsStr::new("--emit"),
            OsStr::new("json"),
        ],
    )?
    .assert()
    .success();
    run_offline_canon(
        &fixture,
        [
            OsStr::new("package"),
            OsStr::new("inspect"),
            fixture.archive.as_os_str(),
            OsStr::new("--emit"),
            OsStr::new("json"),
        ],
    )?
    .assert()
    .success();
    run_offline_canon(
        &fixture,
        [
            OsStr::new("package"),
            OsStr::new("verify"),
            fixture.archive.as_os_str(),
            OsStr::new("--emit"),
            OsStr::new("json"),
        ],
    )?
    .assert()
    .success();
    run_offline_canon(
        &fixture,
        [
            fixture.input_csv.as_os_str(),
            OsStr::new("--registry"),
            fixture.registry.as_os_str(),
            OsStr::new("--column"),
            OsStr::new("id"),
            OsStr::new("--emit"),
            OsStr::new("json"),
            OsStr::new("--no-witness"),
        ],
    )?
    .assert()
    .success();

    assert_eq!(snapshot_tree(&fixture.observed_root)?, before);
    Ok(())
}

#[test]
fn owned_output_mutations_do_not_change_input_fixtures() -> Result<(), Box<dyn Error>> {
    let fixture = SafetyFixture::new()?;
    let registry_before = snapshot_tree(&fixture.registry)?;
    let package_before = snapshot_tree(&fixture.package_root)?;

    let outputs = fixture.temp.path().join("outputs");
    fs::create_dir_all(&outputs)?;
    let seed = outputs.join("registry.csv");
    run_offline_canon(
        &fixture,
        [
            OsStr::new("registry"),
            OsStr::new("export"),
            OsStr::new("--format"),
            OsStr::new("dbt-seed"),
            OsStr::new("--registry"),
            fixture.registry.as_os_str(),
            OsStr::new("--out"),
            seed.as_os_str(),
            OsStr::new("--namespace"),
            OsStr::new("people"),
            OsStr::new("--emit"),
            OsStr::new("json"),
        ],
    )?
    .assert()
    .success();

    let unpacked = outputs.join("unpacked");
    fs::create_dir(&unpacked)?;
    run_offline_canon(
        &fixture,
        [
            OsStr::new("package"),
            OsStr::new("unpack"),
            fixture.archive.as_os_str(),
            OsStr::new("--target"),
            unpacked.as_os_str(),
            OsStr::new("--emit"),
            OsStr::new("summary"),
        ],
    )?
    .assert()
    .success();

    assert!(seed.exists());
    assert!(unpacked.join("package.json").exists());
    assert_eq!(snapshot_tree(&fixture.registry)?, registry_before);
    assert_eq!(snapshot_tree(&fixture.package_root)?, package_before);
    assert_all_paths_under(&outputs, &outputs)?;
    Ok(())
}

#[test]
fn concurrent_publication_subprocesses_preserve_one_linear_head() -> Result<(), Box<dyn Error>> {
    if env::var_os(WORKER_ENV).is_some() {
        return Ok(());
    }

    let temp = TempDir::new()?;
    let backend = FilesystemPublicationBackend::new(temp.path());
    assert_eq!(backend.root(), temp.path());
    assert!(!backend.capabilities().requires_network);
    let base_bytes = publication_package_bytes("people", "1.0.0", "base");
    let base_receipt = backend.publish(publication_request(genesis(), None, base_bytes.clone()))?;
    let base_ref = publication_candidate_ref(&base_bytes);

    let worker_count = 6;
    let exe = env::current_exe()?;
    let children = (0..worker_count)
        .map(|index| {
            StdCommand::new(&exe)
                .arg("--exact")
                .arg("safety_matrix_publication_worker")
                .arg("--nocapture")
                .env(WORKER_ENV, "1")
                .env(ROOT_ENV, temp.path())
                .env(CHANNEL_ENV, "stable")
                .env(BASE_DIGEST_ENV, &base_ref.content_digest)
                .env(EXPECTED_DIGEST_ENV, &base_receipt.current_channel_digest)
                .env(WORKER_INDEX_ENV, index.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut published = 0;
    let mut conflicts = 0;
    for child in children {
        let output = child.wait_with_output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "worker failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        if stdout.contains("worker_outcome=published") {
            published += 1;
        } else if stdout.contains("worker_outcome=conflict") {
            conflicts += 1;
        } else {
            panic!("worker did not report outcome\nstdout:\n{stdout}\nstderr:\n{stderr}");
        }
    }

    assert_eq!(published, 1);
    assert_eq!(conflicts, worker_count - 1);
    assert!(backend.current_head("stable")?.is_some());
    Ok(())
}

#[test]
fn safety_matrix_publication_worker() {
    if env::var_os(WORKER_ENV).is_none() {
        return;
    }

    let backend = FilesystemPublicationBackend::new(env::var(ROOT_ENV).unwrap());
    let base = PublishedPackageRef {
        package_id: "people".to_string(),
        package_version: "1.0.0".to_string(),
        content_digest: env::var(BASE_DIGEST_ENV).unwrap(),
    };
    let expected = env::var(EXPECTED_DIGEST_ENV).ok();
    let worker_index = env::var(WORKER_INDEX_ENV).unwrap();
    let bytes = publication_package_bytes("people", "1.0.1", &format!("worker-{worker_index}"));

    match backend.publish(publication_request(base, expected, bytes)) {
        Ok(receipt) => println!(
            "worker_outcome=published digest={}",
            receipt.current_channel_digest
        ),
        Err(error) if error.kind == PublicationErrorKind::Conflict => {
            println!("worker_outcome=conflict");
        }
        Err(error) => panic!("unexpected publication error: {error:?}"),
    }
}

#[test]
fn ci_wires_focused_safety_matrix_on_linux_and_macos() {
    let ci = include_str!("../.github/workflows/ci.yml");
    assert!(ci.contains("safety-matrix"));
    assert!(ci.contains("cargo test --test safety_matrix"));
    assert!(ci.contains("ubuntu-latest"));
    assert!(ci.contains("macos-latest"));
    assert!(ci.contains("needs: [fmt, clippy, test, safety-matrix, build]"));
}

fn enforce_network_policy(
    policy: NetworkPolicy,
    declaration: &operator::CommandSafetyDeclaration,
) -> Result<(), HarnessRefusal> {
    match (policy, declaration.network) {
        (NetworkPolicy::Deny, NetworkClass::ExplicitExternalProvider) => {
            Err(HarnessRefusal::NetworkDeniedBeforeSpawn {
                command: declaration.command.to_string(),
            })
        }
        (NetworkPolicy::Deny, NetworkClass::Offline | NetworkClass::DeniedByDefault) => Ok(()),
    }
}

struct SafetyFixture {
    temp: TempDir,
    observed_root: PathBuf,
    registry: PathBuf,
    input_csv: PathBuf,
    package_root: PathBuf,
    archive: PathBuf,
}

impl SafetyFixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let temp = TempDir::new()?;
        let observed_root = temp.path().join("observed");
        fs::create_dir_all(&observed_root)?;

        let registry = observed_root.join("registry");
        build_registry_fixture(&registry)?;

        let input_csv = observed_root.join("input.csv");
        fs::write(&input_csv, b"id\nALPHA\n")?;

        let package_root = observed_root.join("package-root");
        let package_bytes = canonical_package_bytes(package_json());
        write_package_root(&package_root, &package_bytes)?;

        let archive = observed_root.join("demo.canonpkg");
        let mut pack = base_canon_command(temp.path())?;
        pack.args([
            "package".as_ref(),
            "pack".as_ref(),
            "--root".as_ref(),
            package_root.as_os_str(),
            "--package".as_ref(),
            package_root.join("package.json").as_os_str(),
            "--out".as_ref(),
            archive.as_os_str(),
        ]);
        pack.assert().success();

        Ok(Self {
            temp,
            observed_root,
            registry,
            input_csv,
            package_root,
            archive,
        })
    }
}

fn run_offline_canon<I, S>(fixture: &SafetyFixture, args: I) -> Result<Command, Box<dyn Error>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = base_canon_command(fixture.temp.path())?;
    command.args(args);
    Ok(command)
}

fn base_canon_command(root: &Path) -> Result<Command, Box<dyn Error>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    let home = root.join("home");
    fs::create_dir_all(&home)?;
    command
        .current_dir(root)
        .env("HOME", &home)
        .env("CANON_REGISTRY_INDEX_MODE", "no-cache")
        .env("CANON_SAFETY_MATRIX_NETWORK", "deny")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .env("NO_PROXY", "*")
        .env("EPISTEMIC_WITNESS", root.join("witness.jsonl"));
    Ok(command)
}

fn build_registry_fixture(path: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(path)?;
    fs::write(
        path.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "people",
            "version": "1.0.0",
            "description": "safety matrix fixture",
            "updated": "2026-07-10",
            "entry_count": 1
        }))?,
    )?;
    fs::write(
        path.join("aliases.json"),
        serde_json::to_vec_pretty(&json!([
            {
                "input": "ALPHA",
                "canonical_id": "PPL-001",
                "canonical_type": "person",
                "rule_id": "MANUAL"
            }
        ]))?,
    )?;
    Ok(())
}

fn package_json() -> Value {
    json!({
        "schema_version": "canon.strategy.package.v1",
        "package_id": "pkg.safety.demo",
        "package_version": "1.0.0",
        "content_digest": "",
        "license_expression": "MIT",
        "capabilities": ["read_registry"],
        "dependency_references": [],
        "provenance": {
            "source": "safety-matrix",
            "revision": "0d6e703"
        }
    })
}

fn canonical_package_bytes(mut value: Value) -> Vec<u8> {
    value["content_digest"] = Value::String(String::new());
    let digest = format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(&value).unwrap()).to_hex()
    );
    value["content_digest"] = Value::String(digest);
    serde_json::to_vec(&value).unwrap()
}

fn write_package_root(root: &Path, package_bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    write_file(root, "README.md", b"safety matrix\n")?;
    write_file(root, "bin/run.sh", b"#!/bin/sh\n")?;
    write_file(root, "data/unicode-cafe.txt", b"cafe\n")?;
    write_file(root, "package.json", package_bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("bin/run.sh"), fs::Permissions::from_mode(0o755))?;
        fs::set_permissions(root.join("README.md"), fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

fn write_file(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn snapshot_tree(root: &Path) -> Result<BTreeMap<String, MetadataSnapshot>, Box<dyn Error>> {
    let mut snapshot = BTreeMap::new();
    collect_snapshot(root, root, &mut snapshot)?;
    Ok(snapshot)
}

fn collect_snapshot(
    root: &Path,
    path: &Path,
    snapshot: &mut BTreeMap<String, MetadataSnapshot>,
) -> Result<(), Box<dyn Error>> {
    let mut children = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.path());
    for child in children {
        let child_path = child.path();
        let metadata = fs::symlink_metadata(&child_path)?;
        let relative = child_path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let file_type = metadata.file_type();
        let kind = if file_type.is_file() {
            EntryKind::File
        } else if file_type.is_dir() {
            EntryKind::Directory
        } else if file_type.is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::Other
        };
        let content_digest = if file_type.is_file() {
            Some(format!(
                "blake3:{}",
                blake3::hash(&fs::read(&child_path)?).to_hex()
            ))
        } else {
            None
        };
        snapshot.insert(
            relative,
            MetadataSnapshot {
                kind,
                len: metadata.len(),
                readonly: metadata.permissions().readonly(),
                modified_nanos: metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos()),
                mode: file_mode(&metadata),
                content_digest,
            },
        );
        if file_type.is_dir() {
            collect_snapshot(root, &child_path, snapshot)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn file_mode(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

fn assert_all_paths_under(root: &Path, allowed_root: &Path) -> Result<(), Box<dyn Error>> {
    for path in snapshot_tree(root)?.keys() {
        let absolute = root.join(path);
        assert!(
            absolute.starts_with(allowed_root),
            "{} escaped {}",
            absolute.display(),
            allowed_root.display()
        );
    }
    Ok(())
}

fn digest(seed: &str) -> String {
    format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex())
}

fn genesis() -> PublishedPackageRef {
    PublishedPackageRef {
        package_id: "people".to_string(),
        package_version: "0.0.0".to_string(),
        content_digest: digest("genesis"),
    }
}

fn publication_request(
    base: PublishedPackageRef,
    expected_channel_digest: Option<String>,
    bytes: Vec<u8>,
) -> PublicationRequest {
    PublicationRequest {
        channel: "stable".to_string(),
        expected_base: base,
        expected_channel_digest,
        candidate_package_bytes: bytes,
    }
}

fn publication_package_bytes(package_id: &str, version: &str, payload: &str) -> Vec<u8> {
    let mut value = json!({
        "schema_version": "canon.registry.package.v1",
        "registry": {
            "id": package_id,
            "version": version
        },
        "content_digest": "",
        "payload": payload
    });
    let content_digest = {
        let mut digest_view = value.clone();
        digest_view["content_digest"] = Value::String(String::new());
        format!(
            "blake3:{}",
            blake3::hash(&serde_json::to_vec(&digest_view).unwrap()).to_hex()
        )
    };
    value["content_digest"] = Value::String(content_digest);
    serde_json::to_vec(&value).unwrap()
}

fn publication_candidate_ref(bytes: &[u8]) -> PublishedPackageRef {
    let value: Value = serde_json::from_slice(bytes).unwrap();
    PublishedPackageRef {
        package_id: value["registry"]["id"].as_str().unwrap().to_string(),
        package_version: value["registry"]["version"].as_str().unwrap().to_string(),
        content_digest: value["content_digest"].as_str().unwrap().to_string(),
    }
}
