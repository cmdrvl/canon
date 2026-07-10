#![forbid(unsafe_code)]

#[path = "../src/extensions/ontology.rs"]
mod ontology;

use ontology::{
    CANON_EXTENSION_ONTOLOGY_VERSION, OntologyDocumentationRef, OntologyErrorCode,
    OntologyObjectClass, OntologyPackage, OntologyPackageCompatibility, OntologyTypeRef,
    canonical_package_bytes, finalize_package, ontology_package_digest, package_compatibility,
    resolve_type_ref, validate_package_for_execution,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.extension.ontology.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/ontology.rs");

#[test]
fn schema_declares_domain_neutral_extension_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_EXTENSION_ONTOLOGY_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_EXTENSION_ONTOLOGY_VERSION
    );
    assert_eq!(
        schema["$defs"]["type_ref"]["properties"]["package_digest"]["$ref"],
        "#/$defs/blake3_hash"
    );
    assert_eq!(
        schema["$defs"]["opaque_type_id"]["pattern"],
        "^[a-z0-9][a-z0-9._-]*:[a-z0-9][a-z0-9._-]*$"
    );
    assert_eq!(
        schema["x-canon-contract"]["compatibility_rule"],
        "same_package_id_and_same_semver_major"
    );
    assert_eq!(
        schema["x-canon-contract"]["domain_specific_core_enums_forbidden"],
        true
    );
}

#[test]
fn unknown_valid_ontology_package_works_without_enum_changes() {
    let package = invented_package("pkg.synthetic", "1.2.3");
    let finalized = finalize_package(package).expect("package finalizes");
    let digest = ontology_package_digest(&finalized).expect("digest computes");

    let reference = OntologyTypeRef {
        package_digest: digest.clone(),
        type_id: "pkg.synthetic:record_bundle".to_string(),
    };
    let resolved = resolve_type_ref(&finalized, &reference).expect("type resolves");
    assert_eq!(resolved.type_id, "pkg.synthetic:record_bundle");
    assert_eq!(resolved.label, "Record Bundle");

    let validated =
        validate_package_for_execution(&finalized, std::slice::from_ref(&reference)).unwrap();
    assert_eq!(validated, digest);
}

#[test]
fn label_only_minor_updates_are_same_major_compatible() {
    let locked = invented_package("pkg.synthetic", "1.2.3");
    let locked = finalize_package(locked).expect("locked package finalizes");
    let locked_digest = ontology_package_digest(&locked).expect("locked digest computes");
    let reference = OntologyTypeRef {
        package_digest: locked_digest,
        type_id: "pkg.synthetic:actor_cluster".to_string(),
    };

    let mut candidate = invented_package("pkg.synthetic", "1.3.0");
    candidate.object_classes[1].label = "Actor Cluster v2 Label".to_string();
    let candidate = finalize_package(candidate).expect("candidate finalizes");

    assert_eq!(
        package_compatibility(&locked, &candidate, &[reference]).expect("same major compatible"),
        OntologyPackageCompatibility::CompatibleSameMajor
    );
}

#[test]
fn digest_or_type_mismatch_fails_before_evidence_execution() {
    let package =
        finalize_package(invented_package("pkg.synthetic", "1.2.3")).expect("package finalizes");

    let digest_error = validate_package_for_execution(
        &package,
        &[OntologyTypeRef {
            package_digest: sample_hash('f'),
            type_id: "pkg.synthetic:record_bundle".to_string(),
        }],
    )
    .expect_err("wrong digest must fail");
    assert_eq!(digest_error.code, OntologyErrorCode::CompatibilityPolicy);

    let missing_type_error = validate_package_for_execution(
        &package,
        &[OntologyTypeRef {
            package_digest: ontology_package_digest(&package).unwrap(),
            type_id: "pkg.synthetic:missing_type".to_string(),
        }],
    )
    .expect_err("missing type must fail");
    assert_eq!(missing_type_error.code, OntologyErrorCode::MissingType);
}

#[test]
fn major_version_change_is_incompatible() {
    let locked = finalize_package(invented_package("pkg.synthetic", "1.2.3"))
        .expect("locked package finalizes");
    let candidate = finalize_package(invented_package("pkg.synthetic", "2.0.0"))
        .expect("candidate package finalizes");
    let locked_digest = ontology_package_digest(&locked).expect("locked digest computes");

    let error = package_compatibility(
        &locked,
        &candidate,
        &[OntologyTypeRef {
            package_digest: locked_digest,
            type_id: "pkg.synthetic:record_bundle".to_string(),
        }],
    )
    .expect_err("major version change must fail");
    assert_eq!(error.code, OntologyErrorCode::CompatibilityPolicy);
}

#[test]
fn shuffled_packages_canonicalize_to_identical_bytes() {
    let left =
        finalize_package(invented_package("pkg.synthetic", "1.2.3")).expect("left finalizes");
    let right = finalize_package(shuffled_invented_package("pkg.synthetic", "1.2.3"))
        .expect("right finalizes");

    let left_bytes = canonical_package_bytes(&left).expect("left serializes");
    let right_bytes = canonical_package_bytes(&right).expect("right serializes");
    assert_eq!(left_bytes, right_bytes);
    assert_eq!(
        ontology_package_digest(&left).unwrap(),
        ontology_package_digest(&right).unwrap()
    );
}

#[test]
fn malicious_documentation_paths_are_rejected() {
    let mut package = invented_package("pkg.synthetic", "1.2.3");
    package.documentation[0].uri = "../secrets.md".to_string();
    let error = finalize_package(package).expect_err("traversal path must fail");
    assert_eq!(error.code, OntologyErrorCode::ArtifactContract);
}

#[test]
fn source_scan_keeps_domain_vocabulary_out_of_core_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "tranche", "servicer", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "ontology module should not embed domain term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "ontology schema should not embed domain term {banned}"
        );
    }
}

fn invented_package(package_id: &str, package_version: &str) -> OntologyPackage {
    OntologyPackage {
        version: String::new(),
        package_id: package_id.to_string(),
        package_version: package_version.to_string(),
        object_classes: vec![
            OntologyObjectClass {
                type_id: format!("{package_id}:record_bundle"),
                label: "Record Bundle".to_string(),
                parent_type_ids: vec![],
                display_groups: vec!["review".to_string(), "analysis".to_string()],
                canonical_id_policy_ref: "policy.synthetic.record_bundle".to_string(),
                allowed_identifier_namespace_refs: vec![
                    "namespace.synthetic.external".to_string(),
                    "namespace.synthetic.local".to_string(),
                ],
                allowed_vocabulary_refs: vec!["vocabulary.synthetic.review".to_string()],
                temporal_behavior_ref: "temporal.optional".to_string(),
                documentation_refs: vec!["docs/ontology.md".to_string()],
            },
            OntologyObjectClass {
                type_id: format!("{package_id}:actor_cluster"),
                label: "Actor Cluster".to_string(),
                parent_type_ids: vec![format!("{package_id}:record_bundle")],
                display_groups: vec!["analysis".to_string()],
                canonical_id_policy_ref: "policy.synthetic.actor_cluster".to_string(),
                allowed_identifier_namespace_refs: vec!["namespace.synthetic.external".to_string()],
                allowed_vocabulary_refs: vec![
                    "vocabulary.synthetic.review".to_string(),
                    "vocabulary.synthetic.audit".to_string(),
                ],
                temporal_behavior_ref: "temporal.required".to_string(),
                documentation_refs: vec!["docs/ontology.md".to_string()],
            },
        ],
        documentation: vec![OntologyDocumentationRef {
            label: "Ontology Overview".to_string(),
            uri: "docs/ontology.md".to_string(),
        }],
    }
}

fn shuffled_invented_package(package_id: &str, package_version: &str) -> OntologyPackage {
    let mut package = invented_package(package_id, package_version);
    package.object_classes.reverse();
    package.object_classes[0].allowed_vocabulary_refs.reverse();
    package.object_classes[1].display_groups.reverse();
    package
}

fn sample_hash(hex: char) -> String {
    format!(
        "blake3:{}",
        std::iter::repeat_n(hex, 64).collect::<String>()
    )
}
