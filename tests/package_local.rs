#![forbid(unsafe_code)]

#[path = "../src/distribution/package.rs"]
mod package;

use assert_cmd::Command;
use package::{
    LocalPackageErrorKind, inspect_local_package, pack_local_package, unpack_local_package,
    verify_local_package,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

#[test]
fn equivalent_roots_pack_to_identical_archive_bytes_and_expose_metadata() {
    let package_bytes = canonical_package_bytes(package_json());
    let left = package_root(false, &package_bytes);
    let right = package_root(true, &package_bytes);

    let left_bytes = pack_local_package(left.path(), &package_bytes).expect("left packs");
    let right_bytes = pack_local_package(right.path(), &package_bytes).expect("right packs");
    assert_eq!(left_bytes, right_bytes);

    let inspection = inspect_local_package(&left_bytes).expect("archive inspects");
    assert_eq!(inspection.package.package_id, "pkg.local.demo");
    assert_eq!(inspection.package.package_version, "1.2.3");
    assert_eq!(inspection.licenses, vec!["MIT"]);
    assert_eq!(
        inspection.capabilities,
        vec!["read_registry", "run_transform"]
    );
    assert_eq!(inspection.dependencies[0].id, "pkg.dep");
    assert_eq!(inspection.inventory.len(), 4);
    assert_eq!(inspection.inventory[0].path, "README.md");
    assert_eq!(inspection.inventory[1].path, "bin/run.sh");
    assert_eq!(inspection.inventory[1].mode, 0o755);
    assert_eq!(inspection.inventory[2].path, "data/unicode-cafe.txt");
    assert_eq!(inspection.inventory[3].path, "package.json");

    let verify = verify_local_package(&left_bytes).expect("archive verifies");
    assert_eq!(verify.verified_files, 4);
    assert_eq!(
        verify.package_content_digest,
        inspection.package.content_digest
    );
}

#[test]
fn verify_detects_package_file_mode_and_schema_tampering() {
    let package_bytes = canonical_package_bytes(package_json());
    let root = package_root(false, &package_bytes);
    let archive_bytes = pack_local_package(root.path(), &package_bytes).expect("packs");

    let mut archive: Value = serde_json::from_slice(&archive_bytes).unwrap();
    archive["files"][0]["data_hex"] = Value::String(hex_bytes(b"changed"));
    let tampered = serde_json::to_vec(&archive).unwrap();
    let error = verify_local_package(&tampered).expect_err("file digest tampering fails");
    assert_eq!(error.kind, LocalPackageErrorKind::InvalidContentDigest);

    let mut archive: Value = serde_json::from_slice(&archive_bytes).unwrap();
    archive["files"][0]["descriptor"]["mode"] = Value::from(0o600);
    archive["archive_digest"] = Value::String(archive_digest_for_test(&archive));
    let tampered = serde_json::to_vec(&archive).unwrap();
    let error = verify_local_package(&tampered).expect_err("mode tampering fails");
    assert_eq!(error.kind, LocalPackageErrorKind::InvalidMode);

    let mut package_json: Value = serde_json::from_slice(&package_bytes).unwrap();
    package_json["schema_version"] = Value::String("canon.unknown.package.v9".to_string());
    let invalid_package = canonical_package_bytes(package_json);
    let error =
        pack_local_package(root.path(), &invalid_package).expect_err("schema tampering fails");
    assert_eq!(error.kind, LocalPackageErrorKind::SemanticContract);
}

#[test]
fn unpack_requires_existing_empty_target_and_preserves_verified_contents() {
    let package_bytes = canonical_package_bytes(package_json());
    let root = package_root(false, &package_bytes);
    let archive_bytes = pack_local_package(root.path(), &package_bytes).expect("packs");

    let non_empty = TempDir::new().unwrap();
    fs::write(non_empty.path().join("existing.txt"), b"occupied").unwrap();
    let error =
        unpack_local_package(&archive_bytes, non_empty.path()).expect_err("non-empty target fails");
    assert_eq!(error.kind, LocalPackageErrorKind::NonEmptyTarget);

    let target = TempDir::new().unwrap();
    let verify = unpack_local_package(&archive_bytes, target.path()).expect("unpacks");
    assert_eq!(verify.verified_files, 4);
    assert_eq!(
        fs::read(target.path().join("README.md")).unwrap(),
        b"demo\n"
    );
    assert_eq!(
        fs::read(target.path().join("bin/run.sh")).unwrap(),
        b"#!/bin/sh\n"
    );
    assert_eq!(
        fs::read(target.path().join("package.json")).unwrap(),
        package_bytes
    );
    assert!(verify_local_package(&archive_bytes).is_ok());
}

#[test]
fn pack_and_verify_reject_links_path_traversal_and_duplicate_paths() {
    let package_bytes = canonical_package_bytes(package_json());

    let duplicate = package_root(false, &package_bytes);
    fs::create_dir_all(duplicate.path().join("a")).unwrap();
    fs::write(duplicate.path().join("a/b.txt"), b"one").unwrap();
    fs::write(duplicate.path().join("a\\b.txt"), b"two").unwrap();
    let error =
        pack_local_package(duplicate.path(), &package_bytes).expect_err("duplicate paths fail");
    assert_eq!(error.kind, LocalPackageErrorKind::DuplicatePath);

    let mut archive: Value = serde_json::from_slice(
        &pack_local_package(package_root(false, &package_bytes).path(), &package_bytes).unwrap(),
    )
    .unwrap();
    archive["files"][0]["descriptor"]["path"] = Value::String("../escape.txt".to_string());
    archive["archive_digest"] = Value::String(archive_digest_for_test(&archive));
    let error = inspect_local_package(&serde_json::to_vec(&archive).unwrap())
        .expect_err("path traversal fails");
    assert_eq!(error.kind, LocalPackageErrorKind::PathTraversal);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let link_root = package_root(false, &package_bytes);
        symlink(
            link_root.path().join("README.md"),
            link_root.path().join("linked-readme"),
        )
        .unwrap();
        let error =
            pack_local_package(link_root.path(), &package_bytes).expect_err("symlink is rejected");
        assert_eq!(error.kind, LocalPackageErrorKind::LinkRejected);

        let hardlink_root = package_root(false, &package_bytes);
        fs::hard_link(
            hardlink_root.path().join("README.md"),
            hardlink_root.path().join("hard-readme"),
        )
        .unwrap();
        let error = pack_local_package(hardlink_root.path(), &package_bytes)
            .expect_err("hardlink is rejected");
        assert_eq!(error.kind, LocalPackageErrorKind::HardLinkRejected);
    }
}

#[test]
fn cli_pack_inspect_verify_and_unpack_round_trip() {
    let package_bytes = canonical_package_bytes(package_json());
    let root = package_root(false, &package_bytes);
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("pkg.canonpkg");

    canon_command()
        .arg("package")
        .arg("pack")
        .arg("--root")
        .arg(root.path())
        .arg("--package")
        .arg(root.path().join("package.json"))
        .arg("--out")
        .arg(&archive)
        .assert()
        .success();

    let inspect = canon_command()
        .arg("package")
        .arg("inspect")
        .arg(&archive)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspect: Value = serde_json::from_slice(&inspect).unwrap();
    assert_eq!(inspect["package"]["package_id"], "pkg.local.demo");
    assert_eq!(inspect["inventory"].as_array().unwrap().len(), 4);

    let verify = canon_command()
        .arg("package")
        .arg("verify")
        .arg(&archive)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let verify: Value = serde_json::from_slice(&verify).unwrap();
    assert_eq!(verify["verified_files"], 4);

    let target = temp.path().join("unpacked");
    fs::create_dir(&target).unwrap();
    canon_command()
        .arg("package")
        .arg("unpack")
        .arg(&archive)
        .arg("--target")
        .arg(&target)
        .arg("--emit")
        .arg("summary")
        .assert()
        .success();

    assert_eq!(
        fs::read(target.join("package.json")).unwrap(),
        package_bytes
    );
}

#[test]
fn cli_pack_reports_typed_refusals_for_pretty_and_malformed_package_json() {
    let canonical_bytes = canonical_package_bytes(package_json());
    let canonical_value: Value = serde_json::from_slice(&canonical_bytes).unwrap();
    let pretty_package = serde_json::to_string_pretty(&canonical_value)
        .unwrap()
        .into_bytes();
    let pretty_root = package_root(false, &pretty_package);
    let temp = TempDir::new().unwrap();
    let pretty_archive = temp.path().join("pretty.canonpkg");

    let pretty_refusal = package_pack_refusal(
        pretty_root.path(),
        &pretty_root.path().join("package.json"),
        &pretty_archive,
    );
    assert_package_refusal_envelope(&pretty_refusal, "E_PACKAGE_NONCANONICAL");
    assert_eq!(
        pretty_refusal["refusal"]["detail"]["package_error_kind"],
        "non_canonical_package_bytes"
    );
    let pretty_next = pretty_refusal["refusal"]["next_command"].as_str().unwrap();
    assert!(pretty_next.contains("python3 -c"), "{pretty_next}");
    assert!(pretty_next.contains("sort_keys=True"), "{pretty_next}");
    assert!(pretty_next.contains("canon package pack"), "{pretty_next}");
    assert!(!pretty_archive.exists());

    let malformed_root = TempDir::new().unwrap();
    write_file(
        malformed_root.path(),
        "package.json",
        b"{\"schema_version\":",
    );
    let malformed_archive = temp.path().join("malformed.canonpkg");

    let malformed_refusal = package_pack_refusal(
        malformed_root.path(),
        &malformed_root.path().join("package.json"),
        &malformed_archive,
    );
    assert_package_refusal_envelope(&malformed_refusal, "E_PARSE");
    assert_eq!(
        malformed_refusal["refusal"]["detail"]["package_error_kind"],
        "parse"
    );
    assert_ne!(
        pretty_refusal["refusal"]["code"],
        malformed_refusal["refusal"]["code"]
    );
    assert!(!malformed_archive.exists());
}

fn package_pack_refusal(root: &Path, package: &Path, out: &Path) -> Value {
    let output = canon_command()
        .arg("package")
        .arg("pack")
        .arg("--root")
        .arg(root)
        .arg("--package")
        .arg(package)
        .arg("--out")
        .arg(out)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(
        !stderr.starts_with("Error:"),
        "package pack refusal leaked raw stderr: {stderr}"
    );
    serde_json::from_slice(&output.stderr).unwrap()
}

fn assert_package_refusal_envelope(payload: &Value, code: &str) {
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["registry"], Value::Null);
    assert_eq!(payload["summary"], Value::Null);
    assert_eq!(payload["mappings"], Value::Array(vec![]));
    assert_eq!(payload["unresolved"], Value::Array(vec![]));
    assert_eq!(payload["refusal"]["code"], code);
    assert!(payload["refusal"]["message"].is_string());
    assert!(payload["refusal"]["detail"].is_object());
    assert!(
        payload["refusal"]["next_command"]
            .as_str()
            .is_some_and(|command| !command.trim().is_empty())
    );
}

fn package_root(reversed_write_order: bool, package_bytes: &[u8]) -> TempDir {
    let temp = TempDir::new().unwrap();
    if reversed_write_order {
        write_file(temp.path(), "data/unicode-cafe.txt", b"cafe\n");
        write_file(temp.path(), "bin/run.sh", b"#!/bin/sh\n");
        write_file(temp.path(), "README.md", b"demo\n");
    } else {
        write_file(temp.path(), "README.md", b"demo\n");
        write_file(temp.path(), "bin/run.sh", b"#!/bin/sh\n");
        write_file(temp.path(), "data/unicode-cafe.txt", b"cafe\n");
    }
    write_file(temp.path(), "package.json", package_bytes);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            temp.path().join("bin/run.sh"),
            fs::Permissions::from_mode(0o775),
        )
        .unwrap();
        fs::set_permissions(
            temp.path().join("README.md"),
            fs::Permissions::from_mode(0o664),
        )
        .unwrap();
    }

    temp
}

fn write_file(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn package_json() -> Value {
    json!({
        "schema_version": "canon.strategy.package.v1",
        "package_id": "pkg.local.demo",
        "package_version": "1.2.3",
        "content_digest": "",
        "license_expression": "MIT",
        "capabilities": ["run_transform", "read_registry"],
        "dependency_references": [
            {
                "id": "pkg.dep",
                "version": "4.5.6",
                "content_digest": digest('d')
            }
        ],
        "provenance": {
            "source": "unit-test",
            "revision": "abc123"
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

fn archive_digest_for_test(value: &Value) -> String {
    let mut digest_view = value.clone();
    digest_view["archive_digest"] = Value::String(String::new());
    format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(&digest_view).unwrap()).to_hex()
    )
}

fn digest(hex: char) -> String {
    format!("blake3:{}", hex.to_string().repeat(64))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
