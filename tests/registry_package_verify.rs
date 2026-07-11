use canon::{
    registry::package::{
        RegistryPackage, RegistryPackageAttachmentDescriptor, RegistryPackageDependencyReference,
        canonical_package_bytes, compile_registry_package, verify_registry_package,
    },
    registry_lint::{self, RegistryLintProfile},
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry_package/minimal")
}

fn copy_fixture_registry() -> TempDir {
    let temp = TempDir::new().unwrap();
    for name in ["registry.json", "mappings.json"] {
        fs::copy(fixture_dir().join(name), temp.path().join(name)).unwrap();
    }
    temp
}

fn top_level_entries(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_str().unwrap().to_string())
        .collect()
}

fn digest(hex: char) -> String {
    assert!(hex.is_ascii_digit() || ('a'..='f').contains(&hex));
    format!("blake3:{}", hex.to_string().repeat(64))
}

fn package_digest_for_test(package: &RegistryPackage) -> String {
    let mut digest_view = package.clone();
    digest_view.content_digest.clear();
    format!(
        "blake3:{}",
        blake3::hash(&canonical_package_bytes(&digest_view).unwrap()).to_hex()
    )
}

fn finding_codes<T>(findings: &[T], code: fn(&T) -> &str) -> BTreeSet<String> {
    findings
        .iter()
        .map(|finding| code(finding).to_string())
        .collect()
}

#[test]
fn verify_clean_package_is_deterministic_offline_and_does_not_require_trust() {
    let registry = copy_fixture_registry();
    let package = compile_registry_package(registry.path()).unwrap();
    let before = top_level_entries(registry.path());

    let report = verify_registry_package(registry.path(), &package).unwrap();
    let repeat = verify_registry_package(registry.path(), &package).unwrap();

    assert!(report.verified);
    assert!(report.findings.is_empty());
    assert_eq!(report.version, "canon.registry.package.verify.v1");
    assert_eq!(report.summary.checked_files, 2);
    assert_eq!(report.summary.entry_count, 2);
    assert_eq!(report.summary.effective_mapping_count, 2);
    assert_eq!(
        serde_json::to_vec(&report).unwrap(),
        serde_json::to_vec(&repeat).unwrap()
    );
    assert_eq!(before, top_level_entries(registry.path()));
    assert!(!registry.path().join("_index.sqlite").exists());
    assert!(report.render_summary().contains("verified=true"));
}

#[test]
fn verify_detects_local_mapping_tamper_against_package_manifest() {
    let registry = copy_fixture_registry();
    let package = compile_registry_package(registry.path()).unwrap();
    let mut mappings: serde_json::Value =
        serde_json::from_slice(&fs::read(registry.path().join("mappings.json")).unwrap()).unwrap();
    mappings.as_array_mut().unwrap()[0]["canonical_id"] =
        serde_json::Value::String("TAMPERED".to_string());
    fs::write(
        registry.path().join("mappings.json"),
        serde_json::to_vec_pretty(&mappings).unwrap(),
    )
    .unwrap();

    let report = verify_registry_package(registry.path(), &package).unwrap();
    let codes = finding_codes(&report.findings, |finding| finding.code.as_str());

    assert!(!report.verified);
    assert!(codes.contains("descriptor_digest_mismatch"));
    assert!(codes.contains("descriptor_inventory_mismatch"));
    assert!(codes.contains("effective_mappings_mismatch"));
    assert!(codes.contains("package_digest_mismatch"));
}

#[test]
fn verify_rejects_stale_package_counts_and_effective_mappings() {
    let registry = copy_fixture_registry();
    let mut package = compile_registry_package(registry.path()).unwrap();
    package.entry_count += 1;
    package.effective_mapping_count -= 1;
    package.lookup_entries.pop();
    package.content_digest = package_digest_for_test(&package);

    let report = verify_registry_package(registry.path(), &package).unwrap();
    let codes = finding_codes(&report.findings, |finding| finding.code.as_str());

    assert!(!report.verified);
    assert!(codes.contains("entry_count_mismatch"));
    assert!(codes.contains("effective_mapping_count_mismatch"));
    assert!(codes.contains("effective_mappings_mismatch"));
    assert!(codes.contains("package_digest_mismatch"));
}

#[test]
fn verify_and_package_lint_reject_duplicate_mapping_inputs() {
    let registry = TempDir::new().unwrap();
    fs::write(
        registry.path().join("registry.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "dupes",
            "version": "1.0.0",
            "description": "duplicate input registry",
            "updated": "2026-07-10",
            "entry_count": 3
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        registry.path().join("mappings.json"),
        serde_json::to_vec_pretty(&serde_json::json!([
            {"input":"A","canonical_id":"C1","canonical_type":"entity","rule_id":"r1"},
            {"input":"B","canonical_id":"C2","canonical_type":"entity","rule_id":"r1"},
            {"input":"A","canonical_id":"C3","canonical_type":"entity","rule_id":"r2"}
        ]))
        .unwrap(),
    )
    .unwrap();

    let package = compile_registry_package(registry.path()).unwrap();
    let report = verify_registry_package(registry.path(), &package).unwrap();
    let lint = registry_lint::lint(registry.path(), RegistryLintProfile::Package).unwrap();
    let report_codes = finding_codes(&report.findings, |finding| finding.code.as_str());
    let lint_codes = finding_codes(&lint.findings, |finding| finding.code.as_str());

    assert!(!report.verified);
    assert!(report_codes.contains("shadowed_mapping_input"));
    assert!(lint_codes.contains("shadowed_mapping_input"));
    assert!(lint.summary.errors > 0);
}

#[test]
fn package_lint_reports_dependency_pins_signature_references_and_sidecar_scope() {
    let registry = copy_fixture_registry();
    let mut package = compile_registry_package(registry.path()).unwrap();
    package
        .dependency_references
        .push(RegistryPackageDependencyReference {
            id: String::new(),
            version: String::new(),
            content_digest: digest('a'),
        });
    package
        .attachments
        .push(RegistryPackageAttachmentDescriptor {
            path: "signature.sig".to_string(),
            kind: "signature".to_string(),
            content_digest: digest('b'),
            bytes: 0,
        });
    package.content_digest = package_digest_for_test(&package);

    let report = verify_registry_package(registry.path(), &package).unwrap();
    let lint = registry_lint::lint_registry_package(registry.path(), &package).unwrap();
    let report_codes = finding_codes(&report.findings, |finding| finding.code.as_str());
    let lint_codes = finding_codes(&lint.findings, |finding| finding.code.as_str());

    assert!(!report.verified);
    assert!(report_codes.contains("dependency_pin_incomplete"));
    assert!(report_codes.contains("attachment_scope_invalid"));
    assert!(report_codes.contains("signature_reference_empty"));
    assert!(report_codes.contains("declared_file_missing"));
    assert!(lint_codes.contains("dependency_pin_incomplete"));
    assert!(lint_codes.contains("attachment_scope_invalid"));
    assert_eq!(lint.profile, "package");
}
