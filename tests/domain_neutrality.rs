#![forbid(unsafe_code)]

#[path = "../src/extensions/mod.rs"]
mod extensions;

use extensions::profile::{
    EntityEvidenceLanes, EntityNormalizedView, EntityOperatorSpec, EntityPatchNamespaces,
    EntityProfileExecutionRequest, EntityProfileFieldMapping, EntityProfileLimits,
    EntityProfileMode, EntityProfilePackage, LinkDirection, ProfileCapability, ProfileModeKind,
    ProfilePackageRef, ProfilePackageRefKind, build_project_lock_view, finalize_package,
    validate_package_for_execution,
};
use extensions::{
    FORBIDDEN_EXTENSION_DOC_REFERENCES, REQUIRED_NEUTRAL_DOC_REFERENCES, render_doc_scan_report,
    render_source_scan_report, scan_domain_neutral_extension_sources, scan_extension_docs,
    scan_stripped_rust_source,
};
use std::collections::BTreeMap;
use std::path::Path;

const EXTENSIONS_DOCS: &str = include_str!("../docs/EXTENSIONS.md");

#[test]
fn extension_runtime_sources_stay_domain_neutral() {
    let violations = scan_domain_neutral_extension_sources(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("extension runtime source scan should read files");
    eprintln!(
        "checked {} extension runtime source files",
        extensions::DOMAIN_NEUTRAL_EXTENSION_SOURCE_FILES.len()
    );
    assert!(
        violations.is_empty(),
        "domain-neutral runtime scan found leaks:\n{}",
        render_source_scan_report(&violations)
    );
}

#[test]
fn source_scan_flags_runtime_leak_but_ignores_comments() {
    let source = r#"
// loan in a comment must not fail the runtime scanner
/* regab inside a block comment is also ignored */
fn neutral_contract() {
    let stage = "loan";
    let provider = "openfigi";
}
"#;
    let violations = scan_stripped_rust_source(
        "synthetic_runtime_fixture",
        "tests/domain_neutrality.rs.fixture",
        source,
        extensions::FORBIDDEN_EXTENSION_RUNTIME_TERMS,
    );
    let seen_terms = violations
        .iter()
        .map(|violation| violation.term.as_str())
        .collect::<Vec<_>>();
    assert_eq!(seen_terms, vec!["loan", "openfigi"]);
    assert!(
        violations.iter().all(|violation| violation.line >= 4),
        "comment-only lines should not be flagged:\n{}",
        render_source_scan_report(&violations)
    );
}

#[test]
fn extensions_docs_stay_neutral_and_use_synthetic_examples() {
    let violations = scan_extension_docs(
        "docs/EXTENSIONS.md",
        EXTENSIONS_DOCS,
        FORBIDDEN_EXTENSION_DOC_REFERENCES,
    );
    eprintln!(
        "checked docs/EXTENSIONS.md for {} forbidden references",
        FORBIDDEN_EXTENSION_DOC_REFERENCES.len()
    );
    assert!(
        violations.is_empty(),
        "extension docs mention shipped-domain assets or vocabulary:\n{}",
        render_doc_scan_report(&violations)
    );
    for required in REQUIRED_NEUTRAL_DOC_REFERENCES {
        assert!(
            EXTENSIONS_DOCS.contains(required),
            "extension docs should include synthetic neutral example {required}"
        );
    }
}

#[test]
fn two_unrelated_neutral_profile_packages_load_without_code_changes() {
    let alpha = finalize_package(sample_profile_package("pkg.alpha", "alpha_profile"))
        .expect("alpha package should finalize");
    let beta = finalize_package(sample_profile_package("pkg.beta", "beta_profile"))
        .expect("beta package should finalize");

    let alpha_cluster = validate_package_for_execution(
        &alpha,
        &EntityProfileExecutionRequest {
            mode: ProfileModeKind::Cluster,
            source_object_type: "pkg.alpha:record".to_string(),
            target_object_type: None,
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
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "cluster_assignments".to_string(),
                "review_queue".to_string(),
            ],
        },
    )
    .expect("alpha cluster execution request should validate");

    let alpha_link = validate_package_for_execution(
        &alpha,
        &EntityProfileExecutionRequest {
            mode: ProfileModeKind::Link,
            source_object_type: "pkg.alpha:record".to_string(),
            target_object_type: Some("pkg.alpha:record".to_string()),
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
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "link_candidates".to_string(),
                "link_decisions".to_string(),
            ],
        },
    )
    .expect("alpha link execution request should validate");

    let beta_cluster = validate_package_for_execution(
        &beta,
        &EntityProfileExecutionRequest {
            mode: ProfileModeKind::Cluster,
            source_object_type: "pkg.beta:record".to_string(),
            target_object_type: None,
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
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "cluster_assignments".to_string(),
                "review_queue".to_string(),
            ],
        },
    )
    .expect("beta cluster execution request should validate");

    let beta_link = validate_package_for_execution(
        &beta,
        &EntityProfileExecutionRequest {
            mode: ProfileModeKind::Link,
            source_object_type: "pkg.beta:record".to_string(),
            target_object_type: Some("pkg.beta:record".to_string()),
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
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "link_candidates".to_string(),
                "link_decisions".to_string(),
            ],
        },
    )
    .expect("beta link execution request should validate");

    let alpha_lock = build_project_lock_view(&alpha, &[]).expect("alpha lock view should build");
    let beta_lock = build_project_lock_view(&beta, &[]).expect("beta lock view should build");

    assert_eq!(alpha_cluster.mode.mode, ProfileModeKind::Cluster);
    assert_eq!(alpha_link.mode.mode, ProfileModeKind::Link);
    assert_eq!(beta_cluster.mode.mode, ProfileModeKind::Cluster);
    assert_eq!(beta_link.mode.mode, ProfileModeKind::Link);
    assert_eq!(alpha_lock.profile, "pkg.alpha.alpha_profile");
    assert_eq!(beta_lock.profile, "pkg.beta.beta_profile");
}

fn sample_profile_package(namespace: &str, profile_name: &str) -> EntityProfilePackage {
    EntityProfilePackage {
        kind: "entity-profile".to_string(),
        profile: format!("{namespace}.{profile_name}"),
        version: "1.0.0".to_string(),
        entity_type: format!("{namespace}:record"),
        identity_semantics: "canonical_display_label".to_string(),
        canonical_type: "record_id".to_string(),
        required_fields: vec!["observation_id".to_string(), "display_name".to_string()],
        normalized_views: BTreeMap::from([(
            "display_core".to_string(),
            EntityNormalizedView {
                operators: vec![
                    "unicode_fold".to_string(),
                    "lowercase".to_string(),
                    "normalize_whitespace".to_string(),
                ],
            },
        )]),
        evidence: EntityEvidenceLanes {
            support: vec![EntityOperatorSpec {
                op: "exact_view".to_string(),
                view: Some("display_core".to_string()),
                params: BTreeMap::new(),
            }],
            cannot_link: vec![EntityOperatorSpec {
                op: "protected_anchor_conflict".to_string(),
                view: Some("display_core".to_string()),
                params: BTreeMap::new(),
            }],
            relation_hints: vec![EntityOperatorSpec {
                op: "context_alignment".to_string(),
                view: Some("display_core".to_string()),
                params: BTreeMap::new(),
            }],
        },
        patch_namespaces: EntityPatchNamespaces {
            aliases: format!("{namespace}.{profile_name}.aliases"),
            distinct: format!("{namespace}.{profile_name}.distinct"),
            relations: format!("{namespace}.{profile_name}.relations"),
        },
        evidence_policy: sample_ref(
            ProfilePackageRefKind::EvidencePolicy,
            namespace,
            "evidence_policy",
            'a',
        ),
        review_policy: sample_ref(
            ProfilePackageRefKind::ReviewPolicy,
            namespace,
            "review_policy",
            'b',
        ),
        promotion_policy: sample_ref(
            ProfilePackageRefKind::PromotionPolicy,
            namespace,
            "promotion_policy",
            'c',
        ),
        frozen_executable_strategy: sample_ref(
            ProfilePackageRefKind::FrozenExecutableStrategy,
            namespace,
            "strategy",
            'd',
        ),
        ontology_package: sample_ref(
            ProfilePackageRefKind::OntologyPackage,
            namespace,
            "ontology",
            'e',
        ),
        identifier_package: sample_ref(
            ProfilePackageRefKind::IdentifierPackage,
            namespace,
            "identifier",
            'f',
        ),
        vocabulary_package: sample_ref(
            ProfilePackageRefKind::VocabularyPackage,
            namespace,
            "vocabulary",
            '1',
        ),
        evidence_package: sample_ref(
            ProfilePackageRefKind::EvidencePackage,
            namespace,
            "evidence",
            '2',
        ),
        normalization_packages: vec![sample_ref(
            ProfilePackageRefKind::NormalizationPackage,
            namespace,
            "normalization",
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
                field_path: "observation_id".to_string(),
                object_type: format!("{namespace}:record"),
                field_role: "record_key".to_string(),
                normalized_view: None,
                required: true,
            },
            EntityProfileFieldMapping {
                field_path: "display_name".to_string(),
                object_type: format!("{namespace}:record"),
                field_role: "display_name".to_string(),
                normalized_view: Some("display_core".to_string()),
                required: true,
            },
        ],
        execution_modes: vec![
            EntityProfileMode {
                mode: ProfileModeKind::Cluster,
                source_object_type: format!("{namespace}:record"),
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
                field_paths: vec!["observation_id".to_string(), "display_name".to_string()],
                outputs: vec![
                    "prepare_bundle".to_string(),
                    "cluster_assignments".to_string(),
                    "review_queue".to_string(),
                ],
            },
            EntityProfileMode {
                mode: ProfileModeKind::Link,
                source_object_type: format!("{namespace}:record"),
                target_object_type: Some(format!("{namespace}:record")),
                link_direction: Some(LinkDirection::Bidirectional),
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
                field_paths: vec!["observation_id".to_string(), "display_name".to_string()],
                outputs: vec![
                    "prepare_bundle".to_string(),
                    "link_candidates".to_string(),
                    "link_decisions".to_string(),
                ],
            },
        ],
        limits: EntityProfileLimits {
            max_observation_fields: 8,
            max_candidate_pairs: 2000,
            max_outputs: 500,
        },
        expected_outputs: vec![
            "prepare_bundle".to_string(),
            "cluster_assignments".to_string(),
            "review_queue".to_string(),
            "link_candidates".to_string(),
            "link_decisions".to_string(),
        ],
        project_overrides: Vec::new(),
    }
}

fn sample_ref(
    kind: ProfilePackageRefKind,
    namespace: &str,
    suffix: &str,
    hash_char: char,
) -> ProfilePackageRef {
    ProfilePackageRef {
        kind,
        id: format!("{namespace}.{suffix}"),
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
