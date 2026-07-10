#![forbid(unsafe_code)]

#[path = "../src/extensions/relation_policy.rs"]
mod relation_policy;

use relation_policy::{
    CANON_RELATION_POLICY_VERSION, IdentityBoundaryPolicy, MergeDisposition, RelationCyclePolicy,
    RelationMergeRequest, RelationObservation, RelationOrientation, RelationPolicyCompatibility,
    RelationPolicyDefinition, RelationPolicyDocumentationRef, RelationPolicyErrorCode,
    RelationPolicyPackage, RelationPolicyRef, RelationReviewContract, ReviewDecision,
    SuccessionTemporalPolicy, TransitiveMergeGuard, canonical_package_bytes,
    evaluate_merge_request, finalize_package, package_compatibility,
    relation_policy_package_digest, relation_policy_schema_version, resolve_policy_ref,
    validate_package_for_execution,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.relation.policy.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/relation_policy.rs");

#[test]
fn schema_declares_domain_neutral_relation_policy_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_RELATION_POLICY_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_RELATION_POLICY_VERSION
    );
    assert_eq!(
        schema["$defs"]["policy_ref"]["properties"]["package_digest"]["$ref"],
        "#/$defs/blake3_hash"
    );
    assert_eq!(
        schema["$defs"]["opaque_ref"]["pattern"],
        "^[a-z0-9][a-z0-9._-]*:[a-z0-9][a-z0-9._-]*$"
    );
    assert_eq!(
        schema["x-canon-contract"]["review_concepts"],
        serde_json::json!(["same", "distinct", "related", "uncertain"])
    );
    assert_eq!(
        schema["x-canon-contract"]["distinct_writes_cannot_link_separately"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["domain_specific_core_enums_forbidden"],
        true
    );
    assert_eq!(
        relation_policy_schema_version(),
        CANON_RELATION_POLICY_VERSION
    );
}

#[test]
fn unknown_hierarchy_policy_works_without_core_enum_changes() {
    let package =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = relation_policy_package_digest(&package).expect("digest computes");
    let reference = RelationPolicyRef {
        package_digest: digest.clone(),
        policy_id: "pkg.synthetic:parent_child_guard".to_string(),
    };

    let resolved = resolve_policy_ref(&package, &reference).expect("policy resolves");
    assert_eq!(resolved.relation_type_id, "pkg.synthetic:parent_child");
    assert_eq!(
        validate_package_for_execution(&package, std::slice::from_ref(&reference)).unwrap(),
        digest
    );
}

#[test]
fn shared_address_pair_order_is_invariant_and_related_review_stays_relation_only() {
    let package =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = relation_policy_package_digest(&package).expect("digest computes");
    let reference = RelationPolicyRef {
        package_digest: digest,
        policy_id: "pkg.synthetic:shared_address_guard".to_string(),
    };

    let left_right = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:alpha".to_string(),
            candidate_right_id: "org:beta".to_string(),
            explicit_equality_fact: false,
            observations: vec![RelationObservation {
                relation_id: sample_hash('a'),
                left_id: "org:alpha".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:beta".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: None,
                right_valid_from: None,
                review_decision: ReviewDecision::Related,
            }],
        },
    )
    .expect("left-right evaluates");
    let right_left = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:beta".to_string(),
            candidate_right_id: "org:alpha".to_string(),
            explicit_equality_fact: false,
            observations: vec![RelationObservation {
                relation_id: sample_hash('a'),
                left_id: "org:beta".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:alpha".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: None,
                right_valid_from: None,
                review_decision: ReviewDecision::Related,
            }],
        },
    )
    .expect("right-left evaluates");

    assert_eq!(left_right.pair_key, right_left.pair_key);
    assert_eq!(left_right.pair_ids, right_left.pair_ids);
    assert_eq!(
        left_right.disposition,
        MergeDisposition::BlockRelatedDistinct
    );
    assert_eq!(
        left_right.decision_artifact.as_ref().unwrap().artifact_kind,
        "related"
    );
    assert!(left_right.cannot_link_patch.is_none());
}

#[test]
fn temporal_succession_requires_non_overlapping_boundaries_and_never_silently_collapses_identity() {
    let package =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = relation_policy_package_digest(&package).expect("digest computes");
    let reference = RelationPolicyRef {
        package_digest: digest,
        policy_id: "pkg.synthetic:successor_guard".to_string(),
    };

    let valid = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:legacy".to_string(),
            candidate_right_id: "org:successor".to_string(),
            explicit_equality_fact: false,
            observations: vec![RelationObservation {
                relation_id: sample_hash('b'),
                left_id: "org:legacy".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:successor".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: Some("2026-03-31T23:59:59Z".to_string()),
                right_valid_from: Some("2026-04-01T00:00:00Z".to_string()),
                review_decision: ReviewDecision::Related,
            }],
        },
    )
    .expect("valid successor evaluates");
    assert_eq!(valid.disposition, MergeDisposition::TemporalSuccessionOnly);
    assert!(valid.cannot_link_patch.is_none());

    let overlap = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:legacy".to_string(),
            candidate_right_id: "org:successor".to_string(),
            explicit_equality_fact: false,
            observations: vec![RelationObservation {
                relation_id: sample_hash('c'),
                left_id: "org:legacy".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:successor".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: Some("2026-04-01T00:00:00Z".to_string()),
                right_valid_from: Some("2026-04-01T00:00:00Z".to_string()),
                review_decision: ReviewDecision::Related,
            }],
        },
    )
    .expect_err("overlap must fail");
    assert_eq!(overlap.code, RelationPolicyErrorCode::TemporalBoundary);

    let same_without_fact = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:legacy".to_string(),
            candidate_right_id: "org:successor".to_string(),
            explicit_equality_fact: false,
            observations: vec![RelationObservation {
                relation_id: sample_hash('d'),
                left_id: "org:legacy".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:successor".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: Some("2026-03-31T23:59:59Z".to_string()),
                right_valid_from: Some("2026-04-01T00:00:00Z".to_string()),
                review_decision: ReviewDecision::Same,
            }],
        },
    )
    .expect("same review still evaluates");
    assert_eq!(
        same_without_fact.disposition,
        MergeDisposition::NeedsExplicitEqualityFact
    );
}

#[test]
fn graph_cycle_cardinality_and_transitive_merge_pressure_are_policy_driven() {
    let package =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = relation_policy_package_digest(&package).expect("digest computes");
    let reference = RelationPolicyRef {
        package_digest: digest,
        policy_id: "pkg.synthetic:parent_child_guard".to_string(),
    };

    let cycle = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:a".to_string(),
            candidate_right_id: "org:c".to_string(),
            explicit_equality_fact: false,
            observations: vec![
                hierarchy_obs(sample_hash('e'), "org:a", "org:b", ReviewDecision::Related),
                hierarchy_obs(sample_hash('f'), "org:b", "org:c", ReviewDecision::Related),
                hierarchy_obs(sample_hash('g'), "org:c", "org:a", ReviewDecision::Related),
            ],
        },
    )
    .expect_err("cycle must fail");
    assert_eq!(cycle.code, RelationPolicyErrorCode::GraphPolicy);

    let cardinality = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:parent_a".to_string(),
            candidate_right_id: "org:child".to_string(),
            explicit_equality_fact: false,
            observations: vec![
                hierarchy_obs(
                    sample_hash('h'),
                    "org:parent_a",
                    "org:child",
                    ReviewDecision::Related,
                ),
                hierarchy_obs(
                    sample_hash('i'),
                    "org:parent_b",
                    "org:child",
                    ReviewDecision::Related,
                ),
            ],
        },
    )
    .expect_err("cardinality must fail");
    assert_eq!(cardinality.code, RelationPolicyErrorCode::GraphPolicy);

    let transitive = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:grandparent".to_string(),
            candidate_right_id: "org:grandchild".to_string(),
            explicit_equality_fact: false,
            observations: vec![
                hierarchy_obs(
                    sample_hash('j'),
                    "org:grandparent",
                    "org:parent",
                    ReviewDecision::Related,
                ),
                hierarchy_obs(
                    sample_hash('k'),
                    "org:parent",
                    "org:grandchild",
                    ReviewDecision::Related,
                ),
            ],
        },
    )
    .expect("transitive pressure evaluates");
    assert_eq!(
        transitive.disposition,
        MergeDisposition::BlockTransitivePressure
    );
    assert!(transitive.decision_artifact.is_none());
}

#[test]
fn review_lanes_and_cannot_link_patches_stay_independent() {
    let package =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = relation_policy_package_digest(&package).expect("digest computes");
    let reference = RelationPolicyRef {
        package_digest: digest,
        policy_id: "pkg.synthetic:brand_operator_guard".to_string(),
    };

    let distinct = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:brand".to_string(),
            candidate_right_id: "org:operator".to_string(),
            explicit_equality_fact: false,
            observations: vec![RelationObservation {
                relation_id: sample_hash('l'),
                left_id: "org:brand".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:operator".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: None,
                right_valid_from: None,
                review_decision: ReviewDecision::Distinct,
            }],
        },
    )
    .expect("distinct evaluates");
    assert_eq!(
        distinct.disposition,
        MergeDisposition::BlockReviewedDistinct
    );
    assert_eq!(
        distinct.decision_artifact.as_ref().unwrap().artifact_kind,
        "distinct"
    );
    assert!(distinct.cannot_link_patch.is_some());

    let related = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:brand".to_string(),
            candidate_right_id: "org:operator".to_string(),
            explicit_equality_fact: false,
            observations: vec![RelationObservation {
                relation_id: sample_hash('m'),
                left_id: "org:brand".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:operator".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: None,
                right_valid_from: None,
                review_decision: ReviewDecision::Related,
            }],
        },
    )
    .expect("related evaluates");
    assert_eq!(related.disposition, MergeDisposition::BlockRelatedDistinct);
    assert_eq!(
        related.decision_artifact.as_ref().unwrap().artifact_kind,
        "related"
    );
    assert!(related.cannot_link_patch.is_none());

    let uncertain = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:brand".to_string(),
            candidate_right_id: "org:operator".to_string(),
            explicit_equality_fact: false,
            observations: vec![RelationObservation {
                relation_id: sample_hash('n'),
                left_id: "org:brand".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:operator".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: None,
                right_valid_from: None,
                review_decision: ReviewDecision::Uncertain,
            }],
        },
    )
    .expect("uncertain evaluates");
    assert_eq!(uncertain.disposition, MergeDisposition::ReviewRequired);

    let same_with_fact = evaluate_merge_request(
        &package,
        &reference,
        &RelationMergeRequest {
            candidate_left_id: "org:brand".to_string(),
            candidate_right_id: "org:operator".to_string(),
            explicit_equality_fact: true,
            observations: vec![RelationObservation {
                relation_id: sample_hash('o'),
                left_id: "org:brand".to_string(),
                left_type_ref: "types.synthetic:organization".to_string(),
                right_id: "org:operator".to_string(),
                right_type_ref: "types.synthetic:organization".to_string(),
                left_valid_to: None,
                right_valid_from: None,
                review_decision: ReviewDecision::Same,
            }],
        },
    )
    .expect("same evaluates");
    assert_eq!(
        same_with_fact.disposition,
        MergeDisposition::AllowWithExplicitEqualityFact
    );
}

#[test]
fn same_major_updates_are_compatible_and_canonical_bytes_are_stable() {
    let locked = finalize_package(sample_package("pkg.synthetic", "1.2.3"))
        .expect("locked package finalizes");
    let locked_digest = relation_policy_package_digest(&locked).expect("digest computes");
    let reference = RelationPolicyRef {
        package_digest: locked_digest,
        policy_id: "pkg.synthetic:parent_child_guard".to_string(),
    };

    let mut candidate = sample_package("pkg.synthetic", "1.4.0");
    candidate.policies[0].review.decision_artifact_family =
        "artifacts.synthetic:relation_decision_v2".to_string();
    let candidate = finalize_package(candidate).expect("candidate finalizes");
    assert_eq!(
        package_compatibility(&locked, &candidate, &[reference]).expect("compatible same major"),
        RelationPolicyCompatibility::CompatibleSameMajor
    );

    let left =
        finalize_package(sample_package("pkg.synthetic", "1.2.3")).expect("left package finalizes");
    let right = finalize_package(shuffled_sample_package("pkg.synthetic", "1.2.3"))
        .expect("right package finalizes");
    let left_bytes = canonical_package_bytes(&left).expect("left serializes");
    let right_bytes = canonical_package_bytes(&right).expect("right serializes");
    assert_eq!(left_bytes, right_bytes);
}

#[test]
fn malicious_documentation_paths_and_domain_terms_are_kept_out_of_core_contract() {
    let mut package = sample_package("pkg.synthetic", "1.2.3");
    package.documentation[0].uri = "../secrets.md".to_string();
    let error = finalize_package(package).expect_err("traversal path rejects");
    assert_eq!(error.code, RelationPolicyErrorCode::ArtifactContract);

    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "tranche", "servicer", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "relation policy module should not embed domain term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "relation policy schema should not embed domain term {banned}"
        );
    }
}

fn sample_package(package_id: &str, package_version: &str) -> RelationPolicyPackage {
    RelationPolicyPackage {
        version: String::new(),
        package_id: package_id.to_string(),
        package_version: package_version.to_string(),
        policies: vec![
            RelationPolicyDefinition {
                policy_id: format!("{package_id}:parent_child_guard"),
                relation_type_id: format!("{package_id}:parent_child"),
                orientation: RelationOrientation::Directed,
                subject_type_refs: vec!["types.synthetic:organization".to_string()],
                object_type_refs: vec!["types.synthetic:organization".to_string()],
                related_distinct_veto: true,
                identity_boundary: IdentityBoundaryPolicy::ExplicitEqualityFactOnly,
                succession_policy: SuccessionTemporalPolicy::Ignore,
                transitive_merge_guard: TransitiveMergeGuard::Block,
                graph: relation_policy::RelationGraphPolicy {
                    max_objects_per_subject: Some(4),
                    max_subjects_per_object: Some(1),
                    cycle_policy: RelationCyclePolicy::Disallow,
                },
                review: RelationReviewContract {
                    decision_artifact_family: "artifacts.synthetic:relation_decision".to_string(),
                    cannot_link_artifact_family: "artifacts.synthetic:cannot_link_patch"
                        .to_string(),
                    distinct_writes_cannot_link: true,
                },
                documentation_refs: vec!["docs/relation-policy.md".to_string()],
            },
            RelationPolicyDefinition {
                policy_id: format!("{package_id}:brand_operator_guard"),
                relation_type_id: format!("{package_id}:brand_operator"),
                orientation: RelationOrientation::Directed,
                subject_type_refs: vec!["types.synthetic:organization".to_string()],
                object_type_refs: vec!["types.synthetic:organization".to_string()],
                related_distinct_veto: true,
                identity_boundary: IdentityBoundaryPolicy::ExplicitEqualityFactOnly,
                succession_policy: SuccessionTemporalPolicy::Ignore,
                transitive_merge_guard: TransitiveMergeGuard::Review,
                graph: relation_policy::RelationGraphPolicy {
                    max_objects_per_subject: None,
                    max_subjects_per_object: None,
                    cycle_policy: RelationCyclePolicy::Review,
                },
                review: RelationReviewContract {
                    decision_artifact_family: "artifacts.synthetic:relation_decision".to_string(),
                    cannot_link_artifact_family: "artifacts.synthetic:cannot_link_patch"
                        .to_string(),
                    distinct_writes_cannot_link: true,
                },
                documentation_refs: vec!["docs/relation-policy.md".to_string()],
            },
            RelationPolicyDefinition {
                policy_id: format!("{package_id}:successor_guard"),
                relation_type_id: format!("{package_id}:successor"),
                orientation: RelationOrientation::Directed,
                subject_type_refs: vec!["types.synthetic:organization".to_string()],
                object_type_refs: vec!["types.synthetic:organization".to_string()],
                related_distinct_veto: true,
                identity_boundary: IdentityBoundaryPolicy::ExplicitEqualityFactOnly,
                succession_policy: SuccessionTemporalPolicy::RequireNonOverlappingTransition,
                transitive_merge_guard: TransitiveMergeGuard::Review,
                graph: relation_policy::RelationGraphPolicy {
                    max_objects_per_subject: Some(1),
                    max_subjects_per_object: Some(1),
                    cycle_policy: RelationCyclePolicy::Disallow,
                },
                review: RelationReviewContract {
                    decision_artifact_family: "artifacts.synthetic:relation_decision".to_string(),
                    cannot_link_artifact_family: "artifacts.synthetic:cannot_link_patch"
                        .to_string(),
                    distinct_writes_cannot_link: true,
                },
                documentation_refs: vec!["docs/relation-policy.md".to_string()],
            },
            RelationPolicyDefinition {
                policy_id: format!("{package_id}:shared_address_guard"),
                relation_type_id: format!("{package_id}:shared_address"),
                orientation: RelationOrientation::UnorderedPair,
                subject_type_refs: vec!["types.synthetic:organization".to_string()],
                object_type_refs: vec!["types.synthetic:organization".to_string()],
                related_distinct_veto: true,
                identity_boundary: IdentityBoundaryPolicy::None,
                succession_policy: SuccessionTemporalPolicy::Ignore,
                transitive_merge_guard: TransitiveMergeGuard::Review,
                graph: relation_policy::RelationGraphPolicy {
                    max_objects_per_subject: None,
                    max_subjects_per_object: None,
                    cycle_policy: RelationCyclePolicy::Allow,
                },
                review: RelationReviewContract {
                    decision_artifact_family: "artifacts.synthetic:relation_decision".to_string(),
                    cannot_link_artifact_family: "artifacts.synthetic:cannot_link_patch"
                        .to_string(),
                    distinct_writes_cannot_link: true,
                },
                documentation_refs: vec!["docs/relation-policy.md".to_string()],
            },
        ],
        documentation: vec![RelationPolicyDocumentationRef {
            label: "Policy Overview".to_string(),
            uri: "docs/relation-policy.md".to_string(),
        }],
    }
}

fn shuffled_sample_package(package_id: &str, package_version: &str) -> RelationPolicyPackage {
    let mut package = sample_package(package_id, package_version);
    package.policies.reverse();
    package.policies[0].documentation_refs.reverse();
    package.policies[1].subject_type_refs.reverse();
    package
}

fn hierarchy_obs(
    relation_id: String,
    left_id: &str,
    right_id: &str,
    review_decision: ReviewDecision,
) -> RelationObservation {
    RelationObservation {
        relation_id,
        left_id: left_id.to_string(),
        left_type_ref: "types.synthetic:organization".to_string(),
        right_id: right_id.to_string(),
        right_type_ref: "types.synthetic:organization".to_string(),
        left_valid_to: None,
        right_valid_from: None,
        review_decision,
    }
}

fn sample_hash(hex: char) -> String {
    let normalized = char::from_digit(hex.to_digit(36).unwrap_or(10) % 16, 16).unwrap();
    format!(
        "blake3:{}",
        std::iter::repeat_n(normalized, 64).collect::<String>()
    )
}
