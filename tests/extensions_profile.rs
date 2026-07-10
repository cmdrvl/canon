#![forbid(unsafe_code)]

const MODULE_SOURCE: &str = include_str!("../src/extensions/profile.rs");
const SCHEMA_JSON: &str = include_str!("../schemas/canon.entity.profile.v1.schema.json");

mod profile_impl {
    #![allow(dead_code)]

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/extensions/profile.rs"
    ));
}

use profile_impl::{
    AppliedProjectOverride, CANON_ENTITY_PROFILE_PACKAGE_VERSION, EntityEvidenceLanes,
    EntityNormalizedView, EntityOperatorSpec, EntityPatchNamespaces, EntityProfileExecutionRequest,
    EntityProfileFieldMapping, EntityProfileLimits, EntityProfileMode, EntityProfilePackage,
    EntityProfilePackageCompatibility, EntityProfileProjectOverride, LinkDirection,
    ProfileCapability, ProfileErrorCode, ProfileModeKind, ProfilePackageRef, ProfilePackageRefKind,
    build_project_lock_view, canonical_package_bytes, entity_profile_package_digest,
    finalize_package, package_compatibility, validate_package_for_execution,
};
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn schema_declares_portable_profile_package_and_mode_support() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_ENTITY_PROFILE_PACKAGE_VERSION);
    assert_eq!(schema["properties"]["kind"]["const"], "entity-profile");
    assert_eq!(
        schema["properties"]["ontology_package"]["$ref"],
        "#/$defs/ontology_package_ref"
    );
    assert_eq!(
        schema["properties"]["execution_modes"]["items"]["$ref"],
        "#/$defs/execution_mode"
    );
    assert_eq!(
        schema["x-canon-contract"]["cluster_and_link_modes_supported"],
        true
    );
    assert!(
        schema["x-canon-contract"]["portable_package_fields"]
            .as_array()
            .expect("portable field list")
            .iter()
            .any(|value| value == "project_overrides")
    );
}

#[test]
fn portable_profile_supports_cluster_and_directional_link_modes() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let cluster_plan = validate_package_for_execution(
        &package,
        &EntityProfileExecutionRequest {
            mode: ProfileModeKind::Cluster,
            source_object_type: "pkg.synthetic:organization".to_string(),
            target_object_type: None,
            required_capabilities: vec![
                ProfileCapability::Prepare,
                ProfileCapability::Evidence,
                ProfileCapability::SolveCluster,
            ],
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "cluster_assignments".to_string(),
            ],
        },
    )
    .expect("cluster mode validates");
    assert_eq!(cluster_plan.mode.mode, ProfileModeKind::Cluster);

    let link_plan = validate_package_for_execution(
        &package,
        &EntityProfileExecutionRequest {
            mode: ProfileModeKind::Link,
            source_object_type: "pkg.synthetic:organization".to_string(),
            target_object_type: Some("pkg.synthetic:organization".to_string()),
            required_capabilities: vec![
                ProfileCapability::Prepare,
                ProfileCapability::Evidence,
                ProfileCapability::SolveLink,
            ],
            required_outputs: vec!["prepare_bundle".to_string(), "link_decisions".to_string()],
        },
    )
    .expect("link mode validates");
    assert_eq!(link_plan.mode.mode, ProfileModeKind::Link);
    assert_eq!(
        link_plan.mode.link_direction,
        Some(LinkDirection::SourceToTarget)
    );
    assert!(link_plan.package_digest.starts_with("blake3:"));
}

#[test]
fn project_lock_view_exposes_defaults_and_overrides() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let view = build_project_lock_view(
        &package,
        &[
            AppliedProjectOverride {
                key: "candidate_limit".to_string(),
                value: "7500".to_string(),
                project_id: "project.synthetic.alpha".to_string(),
            },
            AppliedProjectOverride {
                key: "review_threshold_basis_points".to_string(),
                value: "9700".to_string(),
                project_id: "project.synthetic.alpha".to_string(),
            },
        ],
    )
    .expect("lock view builds");

    assert_eq!(view.profile, "pkg.synthetic.portable_profile");
    assert_eq!(view.defaults.len(), 2);
    assert_eq!(view.overrides.len(), 2);
    assert!(
        view.defaults
            .iter()
            .any(|entry| { entry.key == "candidate_limit" && entry.project_id.is_none() })
    );
    assert!(view.overrides.iter().any(|entry| {
        entry.key == "candidate_limit"
            && entry.value == "7500"
            && entry.artifact_header_key == "candidate_limit"
            && entry.project_lock_key == "limits.candidate_limit"
            && entry.project_id.as_deref() == Some("project.synthetic.alpha")
    }));
}

#[test]
fn wrong_object_types_unknown_fields_and_missing_capabilities_refuse() {
    let mut bad_field = sample_package();
    bad_field.field_mappings[1].normalized_view = Some("missing_view".to_string());
    let error = finalize_package(bad_field).expect_err("unknown view must refuse");
    assert_eq!(error.code, ProfileErrorCode::UnknownField);

    let package = finalize_package(sample_package()).expect("package finalizes");
    let error = validate_package_for_execution(
        &package,
        &EntityProfileExecutionRequest {
            mode: ProfileModeKind::Cluster,
            source_object_type: "pkg.synthetic:person".to_string(),
            target_object_type: None,
            required_capabilities: vec![ProfileCapability::SolveCluster],
            required_outputs: vec!["cluster_assignments".to_string()],
        },
    )
    .expect_err("wrong object type must refuse");
    assert_eq!(error.code, ProfileErrorCode::WrongObjectType);

    let mut missing_capability = sample_package();
    missing_capability
        .available_capabilities
        .retain(|capability| *capability != ProfileCapability::SolveLink);
    let error =
        finalize_package(missing_capability).expect_err("missing mode capability must refuse");
    assert_eq!(error.code, ProfileErrorCode::MissingCapability);
}

#[test]
fn compatibility_firewall_and_canonical_bytes_are_stable() {
    let locked = finalize_package(sample_package()).expect("locked package finalizes");
    let shuffled = finalize_package(shuffled_sample_package()).expect("shuffled package finalizes");

    assert_eq!(
        canonical_package_bytes(&locked).expect("locked bytes"),
        canonical_package_bytes(&shuffled).expect("shuffled bytes")
    );
    assert_eq!(
        entity_profile_package_digest(&locked).expect("locked digest"),
        entity_profile_package_digest(&shuffled).expect("shuffled digest")
    );
    assert_eq!(
        package_compatibility(&locked, &shuffled).expect("same digest compatible"),
        EntityProfilePackageCompatibility::ExactDigest
    );

    let mut same_major = sample_package();
    same_major.version = "1.4.0".to_string();
    same_major
        .expected_outputs
        .push("project_lock_snapshot".to_string());
    let same_major = finalize_package(same_major).expect("same-major package finalizes");
    assert_eq!(
        package_compatibility(&locked, &same_major).expect("same major compatible"),
        EntityProfilePackageCompatibility::CompatibleSameMajor
    );

    let mut incompatible = sample_package();
    incompatible.ontology_package.content_hash = sample_hash('9');
    let incompatible = finalize_package(incompatible).expect("candidate finalizes");
    let error =
        package_compatibility(&locked, &incompatible).expect_err("digest drift must refuse");
    assert_eq!(error.code, ProfileErrorCode::CompatibilityPolicy);
}

#[test]
fn source_scan_keeps_domain_terms_out_of_profile_package_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "servicer", "tranche", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "profile module should not embed domain term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "profile schema should not embed domain term {banned}"
        );
    }
}

fn sample_package() -> EntityProfilePackage {
    EntityProfilePackage {
        kind: "entity-profile".to_string(),
        profile: "pkg.synthetic.portable_profile".to_string(),
        version: "1.2.3".to_string(),
        entity_type: "pkg.synthetic:organization".to_string(),
        identity_semantics: "canonical_display_label".to_string(),
        canonical_type: "organization_id".to_string(),
        required_fields: vec!["observation_id".to_string(), "raw_name".to_string()],
        normalized_views: BTreeMap::from([
            (
                "anchor_key".to_string(),
                EntityNormalizedView {
                    operators: vec!["ascii_trim_upper".to_string()],
                },
            ),
            (
                "organization_core".to_string(),
                EntityNormalizedView {
                    operators: vec![
                        "unicode_fold".to_string(),
                        "lowercase".to_string(),
                        "normalize_whitespace".to_string(),
                    ],
                },
            ),
        ]),
        evidence: EntityEvidenceLanes {
            support: vec![EntityOperatorSpec {
                op: "exact_view".to_string(),
                view: Some("organization_core".to_string()),
                params: BTreeMap::new(),
            }],
            cannot_link: vec![EntityOperatorSpec {
                op: "protected_anchor_conflict".to_string(),
                view: Some("anchor_key".to_string()),
                params: BTreeMap::new(),
            }],
            relation_hints: vec![EntityOperatorSpec {
                op: "context_alignment".to_string(),
                view: Some("organization_core".to_string()),
                params: BTreeMap::new(),
            }],
        },
        patch_namespaces: EntityPatchNamespaces {
            aliases: "pkg.synthetic.portable_profile.aliases".to_string(),
            distinct: "pkg.synthetic.portable_profile.distinct".to_string(),
            relations: "pkg.synthetic.portable_profile.relations".to_string(),
        },
        evidence_policy: sample_ref(
            ProfilePackageRefKind::EvidencePolicy,
            "pkg.synthetic.evidence_policy",
            'a',
        ),
        review_policy: sample_ref(
            ProfilePackageRefKind::ReviewPolicy,
            "pkg.synthetic.review_policy",
            'b',
        ),
        promotion_policy: sample_ref(
            ProfilePackageRefKind::PromotionPolicy,
            "pkg.synthetic.promotion_policy",
            'c',
        ),
        frozen_executable_strategy: sample_ref(
            ProfilePackageRefKind::FrozenExecutableStrategy,
            "pkg.synthetic.cluster_link_strategy",
            'd',
        ),
        ontology_package: sample_ref(
            ProfilePackageRefKind::OntologyPackage,
            "pkg.synthetic.ontology",
            'e',
        ),
        identifier_package: sample_ref(
            ProfilePackageRefKind::IdentifierPackage,
            "pkg.synthetic.identifiers",
            'f',
        ),
        vocabulary_package: sample_ref(
            ProfilePackageRefKind::VocabularyPackage,
            "pkg.synthetic.vocabulary",
            '1',
        ),
        evidence_package: sample_ref(
            ProfilePackageRefKind::EvidencePackage,
            "pkg.synthetic.evidence",
            '2',
        ),
        normalization_packages: vec![sample_ref(
            ProfilePackageRefKind::NormalizationPackage,
            "pkg.synthetic.normalization.core",
            '3',
        )],
        available_capabilities: vec![
            ProfileCapability::Prepare,
            ProfileCapability::Index,
            ProfileCapability::Block,
            ProfileCapability::Evidence,
            ProfileCapability::SolveCluster,
            ProfileCapability::SolveLink,
            ProfileCapability::Review,
            ProfileCapability::Promote,
            ProfileCapability::Apply,
        ],
        field_mappings: vec![
            EntityProfileFieldMapping {
                field_path: "anchor_id".to_string(),
                object_type: "pkg.synthetic:organization".to_string(),
                field_role: "anchor".to_string(),
                normalized_view: Some("anchor_key".to_string()),
                required: false,
            },
            EntityProfileFieldMapping {
                field_path: "observation_id".to_string(),
                object_type: "pkg.synthetic:organization".to_string(),
                field_role: "record_key".to_string(),
                normalized_view: None,
                required: true,
            },
            EntityProfileFieldMapping {
                field_path: "raw_name".to_string(),
                object_type: "pkg.synthetic:organization".to_string(),
                field_role: "display_name".to_string(),
                normalized_view: Some("organization_core".to_string()),
                required: true,
            },
        ],
        execution_modes: vec![
            EntityProfileMode {
                mode: ProfileModeKind::Link,
                source_object_type: "pkg.synthetic:organization".to_string(),
                target_object_type: Some("pkg.synthetic:organization".to_string()),
                link_direction: Some(LinkDirection::SourceToTarget),
                required_capabilities: vec![
                    ProfileCapability::Prepare,
                    ProfileCapability::Index,
                    ProfileCapability::Block,
                    ProfileCapability::Evidence,
                    ProfileCapability::SolveLink,
                    ProfileCapability::Review,
                    ProfileCapability::Promote,
                    ProfileCapability::Apply,
                ],
                field_paths: vec![
                    "observation_id".to_string(),
                    "raw_name".to_string(),
                    "anchor_id".to_string(),
                ],
                outputs: vec![
                    "prepare_bundle".to_string(),
                    "link_candidates".to_string(),
                    "link_decisions".to_string(),
                ],
            },
            EntityProfileMode {
                mode: ProfileModeKind::Cluster,
                source_object_type: "pkg.synthetic:organization".to_string(),
                target_object_type: None,
                link_direction: None,
                required_capabilities: vec![
                    ProfileCapability::Prepare,
                    ProfileCapability::Index,
                    ProfileCapability::Block,
                    ProfileCapability::Evidence,
                    ProfileCapability::SolveCluster,
                    ProfileCapability::Review,
                    ProfileCapability::Promote,
                    ProfileCapability::Apply,
                ],
                field_paths: vec!["observation_id".to_string(), "raw_name".to_string()],
                outputs: vec![
                    "prepare_bundle".to_string(),
                    "cluster_assignments".to_string(),
                    "review_queue".to_string(),
                ],
            },
        ],
        limits: EntityProfileLimits {
            max_observation_fields: 12,
            max_candidate_pairs: 5000,
            max_outputs: 2000,
        },
        expected_outputs: vec![
            "prepare_bundle".to_string(),
            "cluster_assignments".to_string(),
            "review_queue".to_string(),
            "link_candidates".to_string(),
            "link_decisions".to_string(),
        ],
        project_overrides: vec![
            EntityProfileProjectOverride {
                key: "candidate_limit".to_string(),
                default_value: "5000".to_string(),
                artifact_header_key: "candidate_limit".to_string(),
                project_lock_key: "limits.candidate_limit".to_string(),
            },
            EntityProfileProjectOverride {
                key: "review_threshold_basis_points".to_string(),
                default_value: "9500".to_string(),
                artifact_header_key: "review_threshold_basis_points".to_string(),
                project_lock_key: "review.threshold_basis_points".to_string(),
            },
        ],
    }
}

fn shuffled_sample_package() -> EntityProfilePackage {
    let mut package = sample_package();
    package.available_capabilities.reverse();
    package.normalization_packages.reverse();
    package.field_mappings.reverse();
    package.execution_modes.reverse();
    package.expected_outputs.reverse();
    package.project_overrides.reverse();
    package
        .normalized_views
        .get_mut("organization_core")
        .expect("organization_core view")
        .operators
        .reverse();
    package
}

fn sample_ref(kind: ProfilePackageRefKind, id: &str, hash_char: char) -> ProfilePackageRef {
    ProfilePackageRef {
        kind,
        id: id.to_string(),
        version: "2026.07.10".to_string(),
        content_hash: sample_hash(hash_char),
    }
}

fn sample_hash(hex: char) -> String {
    format!(
        "blake3:{}",
        std::iter::repeat_n(hex, 64).collect::<String>()
    )
}
