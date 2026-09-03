#![forbid(unsafe_code)]

use canon::distribution::mirror::{
    MirrorAttestationInput, MirrorBundle, MirrorErrorKind, MirrorExportRequest,
    MirrorImportRequest, MirrorPackageInput, MirrorPackageRestoreRequest, MirrorTrustRootInput,
    export_mirror_bundle, import_mirror_bundle, restore_mirror_package, verify_mirror_bundle,
};
use canon::distribution::package::{
    inspect_local_package, pack_local_package, verify_local_package,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

#[test]
fn full_mirror_bundle_is_deterministic_and_restores_packages() {
    let fixture = MirrorFixture::new();
    let left = export_mirror_bundle(
        MirrorExportRequest::new(
            vec![fixture.root_digest.clone()],
            vec![
                MirrorPackageInput::new(fixture.root_archive.clone()),
                MirrorPackageInput::new(fixture.dependency_archive.clone()),
            ],
        )
        .with_attestations(vec![MirrorAttestationInput::new(
            fixture.root_digest.clone(),
            b"reviewer=offline\n".to_vec(),
        )])
        .with_trust_roots(vec![MirrorTrustRootInput::new(
            "offline-root",
            b"trust-root-v1\n".to_vec(),
        )]),
    )
    .expect("full export");
    let right = export_mirror_bundle(
        MirrorExportRequest::new(
            vec![fixture.root_digest.clone()],
            vec![
                MirrorPackageInput::new(fixture.dependency_archive.clone()),
                MirrorPackageInput::new(fixture.root_archive.clone()),
            ],
        )
        .with_attestations(vec![MirrorAttestationInput::new(
            fixture.root_digest.clone(),
            b"reviewer=offline\n".to_vec(),
        )])
        .with_trust_roots(vec![MirrorTrustRootInput::new(
            "offline-root",
            b"trust-root-v1\n".to_vec(),
        )]),
    )
    .expect("reordered export");
    assert_eq!(left, right);

    let verification = verify_mirror_bundle(&left).expect("bundle verifies");
    assert_eq!(verification.root_count, 1);
    assert_eq!(verification.included_package_count, 2);
    assert_eq!(verification.external_base_package_count, 0);
    assert_eq!(verification.attestation_count, 1);
    assert_eq!(verification.trust_root_count, 1);
    assert!(verification.verified_package_bytes > 0);

    let cache = TempDir::new().unwrap();
    let import = import_mirror_bundle(MirrorImportRequest {
        bundle_bytes: &left,
        cache_dir: cache.path(),
    })
    .expect("import bundle");
    assert_eq!(import.imported.len(), 2);
    assert_eq!(import.reused_existing_count, 0);
    for package in &import.imported {
        assert!(Path::new(&package.path).exists());
    }
    let reused = import_mirror_bundle(MirrorImportRequest {
        bundle_bytes: &left,
        cache_dir: cache.path(),
    })
    .expect("idempotent import");
    assert_eq!(reused.reused_existing_count, 2);

    let target = TempDir::new().unwrap();
    let restore = restore_mirror_package(MirrorPackageRestoreRequest {
        bundle_bytes: &left,
        package_digest: &fixture.root_digest,
        target_dir: target.path(),
    })
    .expect("restore root");
    let original_verification =
        verify_local_package(&fixture.root_archive).expect("original package verifies");
    assert_eq!(restore.package_digest, fixture.root_digest);
    assert_eq!(restore.verification, original_verification);
    assert_eq!(
        restore.verification.package_content_digest,
        fixture.root_digest
    );
    assert_eq!(
        fs::read(target.path().join("package.json")).unwrap(),
        fixture.root_package_bytes
    );
}

#[test]
fn missing_dependencies_refuse_unless_declared_as_incremental_base() {
    let fixture = MirrorFixture::new();
    let missing = export_mirror_bundle(MirrorExportRequest::new(
        vec![fixture.root_digest.clone()],
        vec![MirrorPackageInput::new(fixture.root_archive.clone())],
    ))
    .expect_err("missing dependency refuses");
    assert_eq!(missing.kind, MirrorErrorKind::MissingAncestor);

    let incremental = export_mirror_bundle(
        MirrorExportRequest::new(
            vec![fixture.root_digest.clone()],
            vec![MirrorPackageInput::new(fixture.root_archive.clone())],
        )
        .incremental_from(vec![fixture.dependency_digest.clone()]),
    )
    .expect("incremental export");
    let verification = verify_mirror_bundle(&incremental).expect("incremental verifies");
    assert_eq!(verification.included_package_count, 1);
    assert_eq!(verification.external_base_package_count, 1);

    let bundle: MirrorBundle = serde_json::from_slice(&incremental).unwrap();
    assert_eq!(
        bundle.base_package_digests,
        vec![fixture.dependency_digest.clone()]
    );
    let base_entry = bundle
        .inventory
        .iter()
        .find(|entry| entry.package_digest == fixture.dependency_digest)
        .expect("base dependency is named in inventory");
    assert!(!base_entry.included);
    assert!(base_entry.external_base);
    assert_eq!(base_entry.blob_digest, None);
    assert_eq!(base_entry.blob_bytes, None);
    assert!(base_entry.dependencies.is_empty());
}

#[test]
fn verify_detects_missing_blob_attestation_and_ancestor_inventory() {
    let fixture = MirrorFixture::new();
    let bundle = fixture.full_bundle();

    let mut missing_blob: MirrorBundle = serde_json::from_slice(&bundle).unwrap();
    missing_blob.blobs.remove(0);
    refresh_bundle_digest(&mut missing_blob);
    let error = verify_mirror_bundle(&serde_json::to_vec(&missing_blob).unwrap())
        .expect_err("missing blob refuses");
    assert_eq!(error.kind, MirrorErrorKind::MissingBlob);

    let mut missing_attestation: MirrorBundle = serde_json::from_slice(&bundle).unwrap();
    missing_attestation.attestations.clear();
    refresh_bundle_digest(&mut missing_attestation);
    let error = verify_mirror_bundle(&serde_json::to_vec(&missing_attestation).unwrap())
        .expect_err("missing attestation refuses");
    assert_eq!(error.kind, MirrorErrorKind::MissingAttestation);

    let mut missing_ancestor: MirrorBundle = serde_json::from_slice(&bundle).unwrap();
    let dependency_digest = fixture.dependency_digest.clone();
    missing_ancestor
        .inventory
        .retain(|entry| entry.package_digest != dependency_digest);
    refresh_bundle_digest(&mut missing_ancestor);
    let error = verify_mirror_bundle(&serde_json::to_vec(&missing_ancestor).unwrap())
        .expect_err("missing ancestor refuses");
    assert_eq!(error.kind, MirrorErrorKind::MissingAncestor);
}

#[test]
fn verify_detects_reordered_storage_and_corrupt_inventory_metadata() {
    let fixture = MirrorFixture::new();
    let bundle = fixture.full_bundle();

    let mut reordered: MirrorBundle = serde_json::from_slice(&bundle).unwrap();
    reordered.inventory.reverse();
    reordered.blobs.reverse();
    refresh_bundle_digest(&mut reordered);
    let error = verify_mirror_bundle(&serde_json::to_vec(&reordered).unwrap())
        .expect_err("reordered storage refuses as noncanonical");
    assert_eq!(error.kind, MirrorErrorKind::NonCanonicalBundle);

    let mut corrupt_inventory: MirrorBundle = serde_json::from_slice(&bundle).unwrap();
    let root_entry = corrupt_inventory
        .inventory
        .iter_mut()
        .find(|entry| entry.package_digest == fixture.root_digest)
        .expect("root entry exists");
    root_entry.package_id = "pkg.other".to_string();
    refresh_bundle_digest(&mut corrupt_inventory);
    let error = verify_mirror_bundle(&serde_json::to_vec(&corrupt_inventory).unwrap())
        .expect_err("corrupt package id refuses");
    assert_eq!(error.kind, MirrorErrorKind::CorruptInventory);
}

#[test]
fn import_refuses_existing_cache_collisions_without_overwrite() {
    let fixture = MirrorFixture::new();
    let bundle = fixture.full_bundle();
    let cache = TempDir::new().unwrap();
    let package_dir = cache.path().join("packages");
    fs::create_dir_all(&package_dir).unwrap();
    let unrelated = package_dir.join("unrelated.canonpkg");
    fs::write(&unrelated, b"keep me").unwrap();

    let import = import_mirror_bundle(MirrorImportRequest {
        bundle_bytes: &bundle,
        cache_dir: cache.path(),
    })
    .expect("first import");
    assert_eq!(fs::read(&unrelated).unwrap(), b"keep me");
    let first_path = import.imported[0].path.clone();
    fs::write(&first_path, b"not the package").unwrap();

    let error = import_mirror_bundle(MirrorImportRequest {
        bundle_bytes: &bundle,
        cache_dir: cache.path(),
    })
    .expect_err("collision refuses");
    assert_eq!(error.kind, MirrorErrorKind::ExistingCacheCollision);
}

struct MirrorFixture {
    root_package_bytes: Vec<u8>,
    root_archive: Vec<u8>,
    root_digest: String,
    dependency_archive: Vec<u8>,
    dependency_digest: String,
}

impl MirrorFixture {
    fn new() -> Self {
        let dependency_package_bytes =
            canonical_package_bytes(package_json("pkg.dep", "1.0.0", Vec::new()));
        let dependency_archive = archive_for(&dependency_package_bytes, "dependency\n");
        let dependency_digest = inspect_local_package(&dependency_archive)
            .unwrap()
            .package
            .content_digest;
        let root_package_bytes = canonical_package_bytes(package_json(
            "pkg.root",
            "2.0.0",
            vec![json!({
                "id": "pkg.dep",
                "version": "1.0.0",
                "content_digest": dependency_digest
            })],
        ));
        let root_archive = archive_for(&root_package_bytes, "root\n");
        let root_digest = inspect_local_package(&root_archive)
            .unwrap()
            .package
            .content_digest;

        Self {
            root_package_bytes,
            root_archive,
            root_digest,
            dependency_archive,
            dependency_digest,
        }
    }

    fn full_bundle(&self) -> Vec<u8> {
        export_mirror_bundle(
            MirrorExportRequest::new(
                vec![self.root_digest.clone()],
                vec![
                    MirrorPackageInput::new(self.root_archive.clone()),
                    MirrorPackageInput::new(self.dependency_archive.clone()),
                ],
            )
            .with_attestations(vec![MirrorAttestationInput::new(
                self.root_digest.clone(),
                b"reviewer=offline\n".to_vec(),
            )]),
        )
        .expect("full bundle")
    }
}

fn archive_for(package_bytes: &[u8], readme: &str) -> Vec<u8> {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("README.md"), readme.as_bytes()).unwrap();
    fs::write(root.path().join("package.json"), package_bytes).unwrap();
    let archive = pack_local_package(root.path(), package_bytes).unwrap();
    verify_local_package(&archive).unwrap();
    archive
}

fn package_json(id: &str, version: &str, dependencies: Vec<Value>) -> Value {
    json!({
        "schema_version": "canon.strategy.package.v1",
        "package_id": id,
        "package_version": version,
        "content_digest": "",
        "license_expression": "MIT",
        "capabilities": ["offline_mirror"],
        "dependency_references": dependencies,
        "provenance": {
            "source": "unit-test",
            "revision": version
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

fn refresh_bundle_digest(value: &mut MirrorBundle) {
    value.bundle_digest.clear();
    let digest = format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(value).unwrap()).to_hex()
    );
    value.bundle_digest = digest;
}
