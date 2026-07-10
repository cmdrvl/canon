#![forbid(unsafe_code)]

#[path = "../src/distribution/oci.rs"]
mod oci;

use oci::{
    ANNOTATION_LAYER_ROLE, ANNOTATION_PACKAGE_DIGEST, ANNOTATION_PACKAGE_SCHEMA,
    ANNOTATION_REF_NAME, CANON_LAYER_ROLE_EXTENSION, CANON_LAYER_ROLE_PRIMARY,
    CANON_OCI_CONFIG_MEDIA_TYPE, CANON_OCI_LAYOUT_VERSION, CANON_VERIFY_EXTENSION_POLICY,
    CanonArtifactClass, CanonPackageBinding, OCI_IMAGE_MANIFEST_MEDIA_TYPE, OciContractErrorCode,
    OciDescriptor, REGISTRY_PACKAGE_SCHEMA_ID, REVIEW_ATTESTATION_SCHEMA_ID,
    STRATEGY_PACKAGE_SCHEMA_ID, build_local_layout, build_manifest, canonical_manifest_bytes,
    payload_media_type, validate_binding, validate_manifest,
};
use serde_json::Value;

const OCI_DOC: &str = include_str!("../docs/OCI_ARTIFACTS.md");
const OCI_SOURCE: &str = include_str!("../src/distribution/oci.rs");

#[test]
fn doc_declares_media_types_digest_binding_and_subject_rules() {
    assert!(OCI_DOC.contains("canon.registry.package.v1"));
    assert!(OCI_DOC.contains("canon.strategy.package.v1"));
    assert!(OCI_DOC.contains("canon.identity.fact.package.v1"));
    assert!(OCI_DOC.contains("canon.extension.<name>.vN"));
    assert!(OCI_DOC.contains("preserve-but-ignore-for-semantic-verify"));
    assert!(OCI_DOC.contains("review attestations"));
    assert!(OCI_DOC.contains("promotion attestations"));
    assert!(OCI_DOC.contains("optional export projections"));
}

#[test]
fn registry_and_strategy_media_types_are_schema_derived() {
    let registry = binding(
        CanonArtifactClass::RegistryPackage,
        REGISTRY_PACKAGE_SCHEMA_ID,
    );
    let strategy = binding(
        CanonArtifactClass::StrategyPackage,
        STRATEGY_PACKAGE_SCHEMA_ID,
    );

    assert_eq!(
        payload_media_type(&registry).unwrap(),
        "application/vnd.cmdrvl.canon.registry.package.v1+json"
    );
    assert_eq!(
        payload_media_type(&strategy).unwrap(),
        "application/vnd.cmdrvl.canon.strategy.package.v1+json"
    );
    validate_binding(&registry).unwrap();
    validate_binding(&strategy).unwrap();
}

#[test]
fn domain_extension_package_supports_unknown_schema_without_core_enum_changes() {
    let binding = binding(
        CanonArtifactClass::DomainExtensionPackage,
        "canon.extension.taxonomy.v9",
    );
    let manifest = build_manifest(
        &binding,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&binding).unwrap(), '1', 128),
        None,
        vec![extension_descriptor(
            "application/vnd.example.third-party.layer.v1+json",
            '2',
            64,
        )],
    )
    .unwrap();

    assert_eq!(manifest.layers.len(), 2);
    assert_eq!(
        manifest.layers[1].annotations[ANNOTATION_LAYER_ROLE],
        CANON_LAYER_ROLE_EXTENSION
    );
    validate_manifest(&manifest, &binding).unwrap();
}

#[test]
fn attestations_require_subjects_and_exports_are_subject_bound() {
    let review = binding(
        CanonArtifactClass::ReviewAttestation,
        REVIEW_ATTESTATION_SCHEMA_ID,
    );
    let error = build_manifest(
        &review,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&review).unwrap(), '3', 90),
        None,
        Vec::new(),
    )
    .expect_err("review attestation without subject must fail");
    assert_eq!(error.code, OciContractErrorCode::ArtifactContract);

    let manifest = build_manifest(
        &review,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&review).unwrap(), '3', 90),
        Some(subject_descriptor('4')),
        Vec::new(),
    )
    .unwrap();
    assert!(manifest.subject.is_some());
}

#[test]
fn manifest_repeats_canonical_package_digest_explicitly_and_verifiably() {
    let binding = binding(
        CanonArtifactClass::RegistryPackage,
        REGISTRY_PACKAGE_SCHEMA_ID,
    );
    let manifest = build_manifest(
        &binding,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&binding).unwrap(), '1', 256),
        None,
        Vec::new(),
    )
    .unwrap();

    assert_eq!(
        manifest.annotations[ANNOTATION_PACKAGE_SCHEMA],
        REGISTRY_PACKAGE_SCHEMA_ID
    );
    assert_eq!(
        manifest.annotations[ANNOTATION_PACKAGE_DIGEST],
        binding.package_digest
    );
    assert_eq!(
        manifest.layers[0].annotations[ANNOTATION_PACKAGE_DIGEST],
        binding.package_digest
    );
    assert_eq!(
        manifest.annotations["io.cmdrvl.canon.verify.extension-policy"],
        CANON_VERIFY_EXTENSION_POLICY
    );
    validate_manifest(&manifest, &binding).unwrap();
}

#[test]
fn reordered_extension_layers_canonicalize_to_identical_manifest_bytes() {
    let binding = binding(
        CanonArtifactClass::DomainExtensionPackage,
        "canon.extension.ontology.v1",
    );
    let left = build_manifest(
        &binding,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&binding).unwrap(), '1', 12),
        None,
        vec![
            extension_descriptor("application/vnd.example.zeta.v1+json", '2', 2),
            extension_descriptor("application/vnd.example.alpha.v1+json", '3', 3),
        ],
    )
    .unwrap();
    let right = build_manifest(
        &binding,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&binding).unwrap(), '1', 12),
        None,
        vec![
            extension_descriptor("application/vnd.example.alpha.v1+json", '3', 3),
            extension_descriptor("application/vnd.example.zeta.v1+json", '2', 2),
        ],
    )
    .unwrap();

    assert_eq!(
        canonical_manifest_bytes(&left).unwrap(),
        canonical_manifest_bytes(&right).unwrap()
    );
}

#[test]
fn duplicate_or_missing_primary_layer_fails() {
    let binding = binding(
        CanonArtifactClass::RegistryPackage,
        REGISTRY_PACKAGE_SCHEMA_ID,
    );
    let mut manifest = build_manifest(
        &binding,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&binding).unwrap(), '1', 12),
        None,
        Vec::new(),
    )
    .unwrap();

    manifest.layers.clear();
    let error = validate_manifest(&manifest, &binding).expect_err("missing primary layer fails");
    assert_eq!(error.code, OciContractErrorCode::ArtifactContract);

    let mut duplicated = build_manifest(
        &binding,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&binding).unwrap(), '1', 12),
        None,
        Vec::new(),
    )
    .unwrap();
    duplicated.layers.push(payload_descriptor(
        &payload_media_type(&binding).unwrap(),
        '5',
        11,
    ));
    duplicated.layers[1].annotations.insert(
        ANNOTATION_LAYER_ROLE.to_string(),
        CANON_LAYER_ROLE_PRIMARY.to_string(),
    );
    let error =
        validate_manifest(&duplicated, &binding).expect_err("duplicate primary layer fails");
    assert_eq!(error.code, OciContractErrorCode::ArtifactContract);
}

#[test]
fn unknown_extension_layers_cannot_bypass_semantic_verify() {
    let binding = binding(
        CanonArtifactClass::RegistryPackage,
        REGISTRY_PACKAGE_SCHEMA_ID,
    );
    let mut manifest = build_manifest(
        &binding,
        config_descriptor('0'),
        payload_descriptor(&payload_media_type(&binding).unwrap(), '1', 12),
        None,
        vec![extension_descriptor(
            "application/vnd.example.third-party.layer.v1+json",
            '2',
            2,
        )],
    )
    .unwrap();

    manifest.layers[0].media_type = "application/vnd.example.not-canon-primary.v1+json".to_string();
    manifest.layers[1].annotations.insert(
        ANNOTATION_PACKAGE_DIGEST.to_string(),
        binding.package_digest.clone(),
    );
    manifest.layers[1].annotations.insert(
        ANNOTATION_PACKAGE_SCHEMA.to_string(),
        binding.package_schema.clone(),
    );

    let error = validate_manifest(&manifest, &binding)
        .expect_err("extension layer must not satisfy primary payload semantics");
    assert_eq!(error.code, OciContractErrorCode::CompatibilityPolicy);
}

#[test]
fn local_oci_layout_records_ref_name_and_manifest_descriptor() {
    let layout = build_local_layout(
        "registry/1.2.3",
        OciDescriptor {
            media_type: OCI_IMAGE_MANIFEST_MEDIA_TYPE.to_string(),
            digest: sample_sha256('8'),
            size: 512,
            annotations: Default::default(),
        },
    )
    .unwrap();

    assert_eq!(layout.image_layout_version, CANON_OCI_LAYOUT_VERSION);
    assert_eq!(layout.index.manifests.len(), 1);
    assert_eq!(
        layout.index.manifests[0].annotations[ANNOTATION_REF_NAME],
        "registry/1.2.3"
    );

    let as_json: Value = serde_json::to_value(&layout).unwrap();
    assert_eq!(
        as_json["index"]["mediaType"],
        oci::OCI_IMAGE_INDEX_MEDIA_TYPE
    );
}

#[test]
fn source_scan_keeps_domain_vocabulary_out_of_oci_contract() {
    let lower = OCI_SOURCE.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "servicer", "tenant_label", "tranche"] {
        assert!(
            !lower.contains(banned),
            "OCI contract should not embed domain term {banned}"
        );
    }
    assert!(OCI_SOURCE.contains(CANON_OCI_CONFIG_MEDIA_TYPE));
}

fn binding(artifact_class: CanonArtifactClass, package_schema: &str) -> CanonPackageBinding {
    CanonPackageBinding {
        artifact_class,
        package_schema: package_schema.to_string(),
        package_id: "fixture-package".to_string(),
        package_version: "1.2.3".to_string(),
        package_digest: sample_blake3('a'),
    }
}

fn config_descriptor(hex: char) -> OciDescriptor {
    OciDescriptor {
        media_type: CANON_OCI_CONFIG_MEDIA_TYPE.to_string(),
        digest: sample_sha256(hex),
        size: 32,
        annotations: Default::default(),
    }
}

fn payload_descriptor(media_type: &str, hex: char, size: u64) -> OciDescriptor {
    OciDescriptor {
        media_type: media_type.to_string(),
        digest: sample_sha256(hex),
        size,
        annotations: Default::default(),
    }
}

fn extension_descriptor(media_type: &str, hex: char, size: u64) -> OciDescriptor {
    OciDescriptor {
        media_type: media_type.to_string(),
        digest: sample_sha256(hex),
        size,
        annotations: Default::default(),
    }
}

fn subject_descriptor(hex: char) -> OciDescriptor {
    OciDescriptor {
        media_type: OCI_IMAGE_MANIFEST_MEDIA_TYPE.to_string(),
        digest: sample_sha256(hex),
        size: 512,
        annotations: Default::default(),
    }
}

fn sample_sha256(hex: char) -> String {
    format!(
        "sha256:{}",
        std::iter::repeat_n(hex, 64).collect::<String>()
    )
}

fn sample_blake3(hex: char) -> String {
    format!(
        "blake3:{}",
        std::iter::repeat_n(hex, 64).collect::<String>()
    )
}
