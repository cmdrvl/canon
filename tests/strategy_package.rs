#![forbid(unsafe_code)]

#[path = "../src/strategy/package.rs"]
mod strategy_package;

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use strategy_package::{
    StrategyPackageAuditMetric, StrategyPackageBuildInput, StrategyPackageCapability,
    StrategyPackageDependencyLockBuildInput, StrategyPackageEntrypointBuildInput,
    StrategyPackageErrorKind, StrategyPackageFixtureBuildInput, StrategyPackageFixtureRole,
    StrategyPackageKind, StrategyPackageProvenance, StrategyPackageRuntimeBuildInput,
    StrategyPackageSelection, StrategyPackageSignatureReference, canonical_package_bytes,
    compile_strategy_package, inspect_strategy_package, parse_strategy_package,
    strategy_package_schema_version, validate_strategy_package, verify_strategy_package,
};
use tempfile::TempDir;

const STRATEGY_PACKAGE_SCHEMA_JSON: &str =
    include_str!("../schemas/canon.strategy.package.v1.schema.json");

#[test]
fn strategy_package_schema_declares_pinned_reproducible_contract() {
    let schema: Value = serde_json::from_str(STRATEGY_PACKAGE_SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], "canon.strategy.package.v1");
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        "canon.strategy.package.v1"
    );
    assert_eq!(
        schema["$defs"]["auditContract"]["properties"]["verification_mode"]["const"],
        "read-only"
    );
    assert_eq!(
        schema["x-canon-contract"]["distinct_from"],
        "canon.registry.package.v1"
    );

    let declared = schema["x-canon-contract"]["declared_descriptor_kinds"]
        .as_array()
        .expect("descriptor kinds array")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        declared,
        vec![
            "package-manifest",
            "runtime-lock",
            "dependency-lock",
            "entrypoint",
            "fixture"
        ]
    );
    assert!(
        schema["x-canon-contract"]["root_escape_policy"]
            .as_str()
            .unwrap()
            .contains("rejected")
    );
    assert_eq!(
        strategy_package_schema_version(),
        "canon.strategy.package.v1"
    );
}

#[test]
fn equivalent_package_roots_pack_to_identical_bytes_and_digest() {
    let left = build_fixture_root(false);
    let right = build_fixture_root(true);

    let left_package =
        compile_strategy_package(left.path(), &fixture_build_input(false)).expect("left packs");
    let right_package =
        compile_strategy_package(right.path(), &fixture_build_input(true)).expect("right packs");

    assert_eq!(left_package.content_digest, right_package.content_digest);

    let left_bytes = canonical_package_bytes(&left_package).expect("left bytes");
    let right_bytes = canonical_package_bytes(&right_package).expect("right bytes");
    assert_eq!(left_bytes, right_bytes);

    let reparsed = parse_strategy_package(&left_bytes).expect("package reparses");
    assert_eq!(reparsed, left_package);
    assert_eq!(
        inspect_strategy_package(&right_bytes).unwrap(),
        right_package
    );
    assert_eq!(reparsed.entrypoints[0].descriptor.path, "bin/transform.py");
    assert_eq!(
        reparsed.fixtures[0].descriptor.path,
        "fixtures/mini/input.jsonl"
    );
}

#[test]
fn verify_detects_manifest_dependency_entrypoint_fixture_and_path_tampering() {
    let package = compile_fixture_package();

    let report =
        verify_strategy_package(package.0.path(), &package.1).expect("initial verify succeeds");
    assert_eq!(report.verified_paths, 6);

    let manifest_tampered = clone_fixture_root(package.0.path());
    fs::write(
        manifest_tampered.path().join("strategy/manifest.json"),
        br#"{"package":"mutated"}"#,
    )
    .unwrap();
    let error = verify_strategy_package(manifest_tampered.path(), &package.1).unwrap_err();
    assert_eq!(error.kind, StrategyPackageErrorKind::InvalidContentDigest);

    let dependency_tampered = clone_fixture_root(package.0.path());
    fs::write(
        dependency_tampered.path().join("locks/uv.lock"),
        b"tampered\n",
    )
    .unwrap();
    let error = verify_strategy_package(dependency_tampered.path(), &package.1).unwrap_err();
    assert_eq!(error.kind, StrategyPackageErrorKind::InvalidContentDigest);

    let entrypoint_tampered = clone_fixture_root(package.0.path());
    fs::write(
        entrypoint_tampered.path().join("bin/transform.py"),
        b"print('tampered')\n",
    )
    .unwrap();
    let error = verify_strategy_package(entrypoint_tampered.path(), &package.1).unwrap_err();
    assert_eq!(error.kind, StrategyPackageErrorKind::InvalidContentDigest);

    let fixture_tampered = clone_fixture_root(package.0.path());
    fs::write(
        fixture_tampered.path().join("fixtures/mini/input.jsonl"),
        b"{\"id\":2}\n",
    )
    .unwrap();
    let error = verify_strategy_package(fixture_tampered.path(), &package.1).unwrap_err();
    assert_eq!(error.kind, StrategyPackageErrorKind::InvalidContentDigest);

    let undeclared_file = clone_fixture_root(package.0.path());
    write_file(
        undeclared_file.path(),
        "generated/runner.sh",
        b"#!/bin/sh\necho sneaky\n",
    );
    let error = verify_strategy_package(undeclared_file.path(), &package.1).unwrap_err();
    assert_eq!(error.kind, StrategyPackageErrorKind::UndeclaredFile);

    let bytes = canonical_package_bytes(&package.1).expect("canonical bytes");
    let mut json: Value = serde_json::from_slice(&bytes).expect("canonical json");
    json["entrypoints"][0]["descriptor"]["path"] = Value::String("../bin/transform.py".to_string());
    let error = parse_strategy_package(&serde_json::to_vec(&json).unwrap()).unwrap_err();
    assert_eq!(
        error.kind,
        StrategyPackageErrorKind::PathTraversalDescriptor
    );
}

#[test]
fn validate_and_verify_are_read_only_and_reject_shape_mismatches() {
    let (temp, package) = compile_fixture_package();
    for path in package_paths() {
        let absolute = temp.path().join(path);
        let mut permissions = fs::metadata(&absolute).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&absolute, permissions).unwrap();
    }

    let report = verify_strategy_package(temp.path(), &package).expect("read-only verify works");
    assert_eq!(report.verified_paths, 6);

    let mut wrong_kind = package.clone();
    wrong_kind.strategy.kind = StrategyPackageKind::TaskTransform;
    let error = validate_strategy_package(&wrong_kind).unwrap_err();
    assert_eq!(error.kind, StrategyPackageErrorKind::SelectionKindMismatch);
}

#[cfg(unix)]
#[test]
fn compile_rejects_symlink_and_hardlink_descriptors() {
    use std::os::unix::fs::symlink;

    let symlink_root = TempDir::new().unwrap();
    write_fixture_files(symlink_root.path());
    let real = symlink_root.path().join("bin/real_transform.py");
    fs::write(&real, b"print('real')\n").unwrap();
    fs::remove_file(symlink_root.path().join("bin/transform.py")).unwrap();
    symlink(&real, symlink_root.path().join("bin/transform.py")).unwrap();

    let error =
        compile_strategy_package(symlink_root.path(), &fixture_build_input(false)).unwrap_err();
    assert_eq!(error.kind, StrategyPackageErrorKind::SymlinkDescriptor);

    let hardlink_root = TempDir::new().unwrap();
    write_fixture_files(hardlink_root.path());
    let outside = hardlink_root.path().join("outside.py");
    fs::write(&outside, b"print('outside')\n").unwrap();
    fs::remove_file(hardlink_root.path().join("bin/transform.py")).unwrap();
    fs::hard_link(&outside, hardlink_root.path().join("bin/transform.py")).unwrap();

    let error =
        compile_strategy_package(hardlink_root.path(), &fixture_build_input(false)).unwrap_err();
    assert_eq!(error.kind, StrategyPackageErrorKind::HardLinkDescriptor);
}

fn compile_fixture_package() -> (TempDir, strategy_package::StrategyPackage) {
    let temp = build_fixture_root(false);
    let package =
        compile_strategy_package(temp.path(), &fixture_build_input(false)).expect("package");
    (temp, package)
}

fn build_fixture_root(use_windows_separators_in_input: bool) -> TempDir {
    let temp = TempDir::new().unwrap();
    write_fixture_files(temp.path());
    if use_windows_separators_in_input {
        let extra = temp.path().join("fixtures/mini");
        fs::write(extra.join("notes.txt"), b"").unwrap();
        fs::remove_file(extra.join("notes.txt")).unwrap();
    }
    temp
}

fn fixture_build_input(use_windows_separators: bool) -> StrategyPackageBuildInput {
    let path = |value: &str| {
        if use_windows_separators {
            value.replace('/', "\\")
        } else {
            value.to_string()
        }
    };

    StrategyPackageBuildInput {
        package_id: "normalize-vendor-extract".to_string(),
        package_version: "1.0.0".to_string(),
        strategy: strategy_package::StrategyPackageStrategy {
            kind: StrategyPackageKind::SchemaTransform,
            selection: StrategyPackageSelection::SchemaTransform {
                schema_fingerprint: "blake3:schema-profile".to_string(),
                skill_hash: "blake3:skill-doctrine".to_string(),
            },
        },
        manifest_path: path("strategy/manifest.json"),
        runtime: StrategyPackageRuntimeBuildInput {
            runtime: "python".to_string(),
            version: "3.12.4".to_string(),
            interface: "cpython-3.12".to_string(),
            path: path("runtime/python-version.txt"),
        },
        dependency_lock: StrategyPackageDependencyLockBuildInput {
            ecosystem: "uv".to_string(),
            path: path("locks/uv.lock"),
        },
        entrypoints: vec![StrategyPackageEntrypointBuildInput {
            name: "transform".to_string(),
            path: path("bin/transform.py"),
            argv: vec!["--emit".to_string(), "json".to_string()],
        }],
        fixtures: vec![
            StrategyPackageFixtureBuildInput {
                suite_id: "mini-suite".to_string(),
                role: StrategyPackageFixtureRole::ExpectedStdout,
                path: path("fixtures/mini/expected.stdout"),
            },
            StrategyPackageFixtureBuildInput {
                suite_id: "mini-suite".to_string(),
                role: StrategyPackageFixtureRole::Input,
                path: path("fixtures/mini/input.jsonl"),
            },
        ],
        provenance: StrategyPackageProvenance {
            source_ref: "git:abc123".to_string(),
            project_ref: "canon/tests/strategy-package".to_string(),
            run_ref: "audit-run-001".to_string(),
            builder_ref: "canon strategy audit".to_string(),
        },
        capabilities: vec![
            StrategyPackageCapability::ReadOnlyVerify,
            StrategyPackageCapability::PinnedDependencies,
            StrategyPackageCapability::NoLiveNetwork,
            StrategyPackageCapability::DeterministicLocalExecution,
            StrategyPackageCapability::AuditFixturesRequired,
        ],
        audit_metrics: vec![
            StrategyPackageAuditMetric {
                name: "deterministic_replay_runs".to_string(),
                expected: 2,
            },
            StrategyPackageAuditMetric {
                name: "fixture_pass_count".to_string(),
                expected: 2,
            },
        ],
        license_expression: "MIT".to_string(),
        signature_references: vec![StrategyPackageSignatureReference {
            kind: "cosign".to_string(),
            reference: "rekor://canon/strategy-package/normalize-vendor-extract".to_string(),
            content_digest: "blake3:signature-record".to_string(),
        }],
    }
}

fn write_fixture_files(root: &Path) {
    write_file(
        root,
        "strategy/manifest.json",
        serde_json::to_vec_pretty(&json!({
            "entrypoint": "bin/transform.py",
            "runtime": "python==3.12.4",
            "lockfile": "locks/uv.lock"
        }))
        .unwrap()
        .as_slice(),
    );
    write_file(root, "runtime/python-version.txt", b"python==3.12.4\n");
    write_file(
        root,
        "locks/uv.lock",
        b"[[package]]\nname = \"canon\"\nversion = \"0.1.0\"\n",
    );
    write_file(root, "bin/transform.py", b"print('stable transform')\n");
    write_file(root, "fixtures/mini/input.jsonl", b"{\"row_id\":1}\n");
    write_file(
        root,
        "fixtures/mini/expected.stdout",
        b"{\"result\":\"ok\"}\n",
    );
}

fn write_file(root: &Path, relative: &str, bytes: &[u8]) {
    let absolute = root.join(relative);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(absolute, bytes).unwrap();
}

fn clone_fixture_root(source: &Path) -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_tree(source, temp.path());
    temp
}

fn copy_tree(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_tree(&path, &target);
        } else {
            fs::copy(path, target).unwrap();
        }
    }
}

fn package_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("strategy/manifest.json"),
        PathBuf::from("runtime/python-version.txt"),
        PathBuf::from("locks/uv.lock"),
        PathBuf::from("bin/transform.py"),
        PathBuf::from("fixtures/mini/input.jsonl"),
        PathBuf::from("fixtures/mini/expected.stdout"),
    ]
}
