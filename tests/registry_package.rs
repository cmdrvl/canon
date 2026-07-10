use canon::registry::{
    REGISTRY_PACKAGE_SCHEMA_VERSION, RegistryPackage, RegistryPackageAttachmentDescriptor,
    RegistryPackageDependencyReference, RegistryPackageDeploymentProjection,
    RegistryPackageDescriptor, RegistryPackageErrorKind, RegistryPackageIdentityRules,
    RegistryPackageLayouts, RegistryPackageRegistryIdentity, canonical_package_bytes,
    compile_registry_package, parse_registry_package, validate_registry_package,
};
use serde_json::Value;
use std::{fs, path::Path};
use tempfile::TempDir;

fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry_package/minimal")
}

fn copy_fixture_registry() -> TempDir {
    let temp = TempDir::new().unwrap();
    for name in ["registry.json", "mappings.json"] {
        fs::copy(fixture_dir().join(name), temp.path().join(name)).unwrap();
    }
    temp
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn base_package() -> RegistryPackage {
    compile_registry_package(fixture_dir().as_path()).expect("fixture package compiles")
}

fn package_digest_for_test(package: &RegistryPackage) -> String {
    let mut digest_view = package.clone();
    digest_view.content_digest.clear();
    format!(
        "blake3:{}",
        blake3::hash(&canonical_package_bytes(&digest_view).unwrap()).to_hex()
    )
}

#[test]
fn registry_package_digest_is_stable_across_path_and_mtime_and_ignores_index_cache() {
    let fixture_package = base_package();

    let copy = copy_fixture_registry();
    let copy_registry_json = copy.path().join("registry.json");
    let same_bytes = fs::read(&copy_registry_json).unwrap();
    fs::write(&copy_registry_json, same_bytes).unwrap();
    fs::write(copy.path().join("_index.sqlite"), b"derived-cache").unwrap();
    let copy_package = compile_registry_package(copy.path()).expect("copied package compiles");

    assert_eq!(fixture_package.content_digest, copy_package.content_digest);
    assert_eq!(fixture_package.entry_count, 2);
    assert_eq!(fixture_package.effective_mapping_count, 2);
    assert_eq!(
        fixture_package.canonical_iri_namespace.as_deref(),
        Some("https://example.test/canon/minimal/")
    );
}

#[test]
fn registry_package_mapping_provenance_and_attachment_changes_change_digest() {
    let original = base_package();

    let mapping_changed = copy_fixture_registry();
    let mut mappings = read_json(&mapping_changed.path().join("mappings.json"));
    mappings.as_array_mut().unwrap()[0]["rule_id"] = Value::String("RULE-2".to_string());
    fs::write(
        mapping_changed.path().join("mappings.json"),
        serde_json::to_vec_pretty(&mappings).unwrap(),
    )
    .unwrap();
    let mapping_package = compile_registry_package(mapping_changed.path()).unwrap();
    assert_ne!(original.content_digest, mapping_package.content_digest);

    let provenance_changed = copy_fixture_registry();
    fs::write(
        provenance_changed.path().join("_build.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": "canon_registry_build.v0",
            "source": "mock",
            "seed_hash": "blake3:seed-1"
        }))
        .unwrap(),
    )
    .unwrap();
    let provenance_package = compile_registry_package(provenance_changed.path()).unwrap();
    assert_ne!(original.content_digest, provenance_package.content_digest);

    let mut attachment_package = original.clone();
    attachment_package
        .attachments
        .push(RegistryPackageAttachmentDescriptor {
            path: "_attachments/audit.json".to_string(),
            kind: "audit".to_string(),
            content_digest: "blake3:audit-payload".to_string(),
            bytes: 42,
        });
    attachment_package.content_digest.clear();
    let error =
        parse_registry_package(&canonical_package_bytes(&attachment_package).unwrap()).unwrap_err();
    assert_eq!(error.kind, RegistryPackageErrorKind::InvalidPackageDigest);
    attachment_package.content_digest = package_digest_for_test(&attachment_package);
    let attachment_bytes = canonical_package_bytes(&attachment_package).unwrap();
    let parsed = parse_registry_package(&attachment_bytes).unwrap();
    assert_ne!(original.content_digest, parsed.content_digest);
}

#[test]
fn registry_package_round_trips_through_canonical_bytes_and_keeps_lookup_entries() {
    let package = base_package();
    let bytes = canonical_package_bytes(&package).expect("canonical bytes");
    let reparsed = parse_registry_package(&bytes).expect("package reparses");

    assert_eq!(package, reparsed);
    assert_eq!(reparsed.lookup_entries.len(), 2);
    assert_eq!(reparsed.lookup_entries[0].input, "AAPL");
    assert_eq!(reparsed.lookup_entries[1].input, "MSFT");
}

#[test]
fn registry_package_refuses_unknown_duplicate_and_path_traversal_descriptors() {
    let package = base_package();

    let mut unknown = package.clone();
    unknown.file_descriptors[0].kind = "mystery".to_string();
    let error = validate_registry_package(&unknown).unwrap_err();
    assert_eq!(error.kind, RegistryPackageErrorKind::UnknownDescriptorKind);

    let mut duplicate = package.clone();
    duplicate.file_descriptors.push(RegistryPackageDescriptor {
        path: "mappings.json".to_string(),
        kind: "mapping".to_string(),
        content_digest: "blake3:duplicate".to_string(),
        bytes: 1,
        entry_count: Some(1),
    });
    let error = validate_registry_package(&duplicate).unwrap_err();
    assert_eq!(
        error.kind,
        RegistryPackageErrorKind::DuplicateDescriptorPath
    );

    let mut traversal = package.clone();
    traversal
        .attachments
        .push(RegistryPackageAttachmentDescriptor {
            path: "../audit.json".to_string(),
            kind: "audit".to_string(),
            content_digest: "blake3:audit".to_string(),
            bytes: 1,
        });
    let error = validate_registry_package(&traversal).unwrap_err();
    assert_eq!(
        error.kind,
        RegistryPackageErrorKind::PathTraversalDescriptor
    );
}

#[test]
fn registry_package_cross_platform_descriptor_order_is_digest_stable() {
    let package = base_package();
    let mut permuted = RegistryPackage {
        schema_version: REGISTRY_PACKAGE_SCHEMA_VERSION.to_string(),
        registry: RegistryPackageRegistryIdentity {
            id: package.registry.id.clone(),
            version: package.registry.version.clone(),
        },
        content_digest: package.content_digest.clone(),
        entry_count: package.entry_count,
        effective_mapping_count: package.effective_mapping_count,
        canonical_iri_namespace: package.canonical_iri_namespace.clone(),
        file_descriptors: package
            .file_descriptors
            .iter()
            .rev()
            .cloned()
            .map(|mut descriptor| {
                descriptor.path = descriptor.path.replace('/', "\\");
                descriptor
            })
            .collect(),
        build_provenance: package.build_provenance.clone(),
        attachments: vec![RegistryPackageAttachmentDescriptor {
            path: "_attachments\\audit.json".to_string(),
            kind: "audit".to_string(),
            content_digest: "blake3:audit".to_string(),
            bytes: 1,
        }],
        dependency_references: vec![RegistryPackageDependencyReference {
            id: "dep-b".to_string(),
            version: "2.0.0".to_string(),
            content_digest: "blake3:dep-b".to_string(),
        }],
        allowed_sidecars: vec![
            "escrow".to_string(),
            "relation".to_string(),
            "signature".to_string(),
            "strategy".to_string(),
            "gold".to_string(),
            "audit".to_string(),
        ],
        deployment_projections: vec![
            RegistryPackageDeploymentProjection {
                kind: "search-index".to_string(),
                first_class: true,
                identity_excluded: true,
            },
            RegistryPackageDeploymentProjection {
                kind: "dbt-seed".to_string(),
                first_class: true,
                identity_excluded: true,
            },
        ],
        lookup_entries: package.lookup_entries.clone().into_iter().rev().collect(),
        identity: RegistryPackageIdentityRules {
            hash_algorithm: "blake3".to_string(),
            descriptor_ordering: "normalized_path_lexicographic".to_string(),
            mapping_precedence: "filename_lexicographic_then_entry_order".to_string(),
            identity_exclusions: vec![
                "secrets".to_string(),
                "provider_credentials".to_string(),
                "derived_caches".to_string(),
                "absolute_paths".to_string(),
                "mtime".to_string(),
                "_index.sqlite".to_string(),
            ],
            secret_material_policy: "never_include_secrets_in_package_manifest".to_string(),
        },
        layouts: RegistryPackageLayouts {
            directory_layout: "registry-package-dir.v1".to_string(),
            archive_layout: "registry-package-archive.v1".to_string(),
            attachment_root: "_attachments/".to_string(),
        },
    };
    permuted.content_digest = package_digest_for_test(&permuted);

    let reparsed = parse_registry_package(&canonical_package_bytes(&permuted).unwrap()).unwrap();
    assert_eq!(reparsed.registry, package.registry);
    assert_eq!(reparsed.file_descriptors[0].path, "mappings.json");
    assert_eq!(reparsed.file_descriptors[1].path, "registry.json");
}
