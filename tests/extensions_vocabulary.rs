#![forbid(unsafe_code)]

#[path = "../src/extensions/vocabulary.rs"]
mod vocabulary;

use serde_json::Value;
use vocabulary::{
    CANON_EXTENSION_VOCABULARY_VERSION, CardinalityHint, IntervalRequirement, RelationDirection,
    RelationFact, RelationInterval, VocabularyDocumentationRef, VocabularyErrorCode,
    VocabularyPackage, VocabularyPackageCompatibility, VocabularyTerm, VocabularyTermKind,
    VocabularyTermRef, canonical_package_bytes, finalize_package, normalize_term_name,
    package_compatibility, resolve_term_ref, validate_package_for_execution,
    validate_relation_fact, vocabulary_package_digest,
};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.extension.vocabulary.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/vocabulary.rs");

#[test]
fn schema_declares_role_and_relationship_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_EXTENSION_VOCABULARY_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_EXTENSION_VOCABULARY_VERSION
    );
    assert_eq!(
        schema["$defs"]["term_ref"]["properties"]["package_digest"]["$ref"],
        "#/$defs/blake3_hash"
    );
    assert_eq!(
        schema["$defs"]["opaque_term_id"]["pattern"],
        "^[a-z0-9][a-z0-9._-]*:[a-z0-9][a-z0-9._-]*$"
    );
    assert_eq!(
        schema["$defs"]["term"]["properties"]["identity_implication"]["const"],
        false
    );
    assert_eq!(
        schema["x-canon-contract"]["compatibility_rule"],
        "same_package_id_and_same_semver_major"
    );
    assert_eq!(
        schema["x-canon-contract"]["identity_implication_default"],
        false
    );
    assert!(
        schema["x-canon-contract"]["exact_lookup_boundary"]
            .as_str()
            .unwrap()
            .contains("separate explicit identity policy")
    );
}

#[test]
fn unknown_namespaced_terms_survive_import_export_without_enum_changes() {
    let package =
        finalize_package(invented_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = vocabulary_package_digest(&package).expect("digest computes");
    let reference = VocabularyTermRef {
        package_digest: digest.clone(),
        term_id: "pkg.synthetic:transfer".to_string(),
    };

    let resolved = resolve_term_ref(&package, &reference).expect("term resolves");
    assert_eq!(resolved.term_id, "pkg.synthetic:transfer");
    assert_eq!(
        resolved.subject_type_refs,
        vec!["types.synthetic:assignment"]
    );

    let validated =
        validate_package_for_execution(&package, std::slice::from_ref(&reference)).unwrap();
    assert_eq!(validated, digest);

    let exported = canonical_package_bytes(&package).expect("package serializes");
    let imported: VocabularyPackage =
        serde_json::from_slice(&exported).expect("canonical package parses");
    let imported = finalize_package(imported).expect("imported package finalizes");
    assert_eq!(
        vocabulary_package_digest(&imported).unwrap(),
        vocabulary_package_digest(&package).unwrap()
    );
}

#[test]
fn role_synonyms_normalize_to_stable_ids_without_resolving_parties() {
    let package =
        finalize_package(invented_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let reference = normalize_term_name(&package, "  DeSk LeAd  ").expect("synonym normalizes");
    assert_eq!(reference.term_id, "pkg.synthetic:assignee");

    let validated = validate_relation_fact(
        &package,
        &RelationFact {
            package_digest: reference.package_digest,
            term_id: reference.term_id,
            subject_id: "person:alex".to_string(),
            subject_type_ref: "types.synthetic:person".to_string(),
            object_id: "queue:west".to_string(),
            object_type_ref: "types.synthetic:queue".to_string(),
            interval: None,
        },
    )
    .expect("role relation validates");

    assert_eq!(validated.term.kind, VocabularyTermKind::Role);
    assert_eq!(validated.relation.subject_id, "person:alex");
    assert_eq!(validated.relation.object_id, "queue:west");
    assert!(!validated.term.identity_implication);
}

#[test]
fn synonym_ambiguity_is_rejected_deterministically() {
    let package =
        finalize_package(ambiguous_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let error = normalize_term_name(&package, "desk lead").expect_err("ambiguity must fail");
    assert_eq!(error.code, VocabularyErrorCode::AmbiguousSynonym);
}

#[test]
fn transfer_and_undirected_relations_enforce_declared_constraints() {
    let package =
        finalize_package(invented_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = vocabulary_package_digest(&package).expect("digest computes");

    let transfer = validate_relation_fact(
        &package,
        &RelationFact {
            package_digest: digest.clone(),
            term_id: "pkg.synthetic:transfer".to_string(),
            subject_id: "assignment:primary".to_string(),
            subject_type_ref: "types.synthetic:assignment".to_string(),
            object_id: "assignment:backup".to_string(),
            object_type_ref: "types.synthetic:assignment".to_string(),
            interval: Some(RelationInterval {
                start_at: "2026-01-01".to_string(),
                end_at: Some("2026-03-31".to_string()),
            }),
        },
    )
    .expect("transfer validates");
    assert_eq!(
        transfer.term.interval_requirement,
        IntervalRequirement::Required
    );
    assert_eq!(transfer.relation.subject_id, "assignment:primary");
    assert_eq!(transfer.relation.object_id, "assignment:backup");

    let coverage_pair = validate_relation_fact(
        &package,
        &RelationFact {
            package_digest: digest,
            term_id: "pkg.synthetic:coverage_pair".to_string(),
            subject_id: "queue:west".to_string(),
            subject_type_ref: "types.synthetic:queue".to_string(),
            object_id: "desk:alpha".to_string(),
            object_type_ref: "types.synthetic:desk".to_string(),
            interval: None,
        },
    )
    .expect("undirected swapped types validate");
    assert_eq!(coverage_pair.term.direction, RelationDirection::Undirected);
}

#[test]
fn invalid_subject_object_types_are_rejected() {
    let package =
        finalize_package(invented_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = vocabulary_package_digest(&package).expect("digest computes");

    let error = validate_relation_fact(
        &package,
        &RelationFact {
            package_digest: digest,
            term_id: "pkg.synthetic:assignee".to_string(),
            subject_id: "queue:west".to_string(),
            subject_type_ref: "types.synthetic:queue".to_string(),
            object_id: "person:alex".to_string(),
            object_type_ref: "types.synthetic:person".to_string(),
            interval: None,
        },
    )
    .expect_err("invalid subject/object types must fail");
    assert_eq!(error.code, VocabularyErrorCode::ConstraintViolation);
}

#[test]
fn cycles_remain_relation_only_context_and_do_not_emit_aliases() {
    let package =
        finalize_package(invented_package("pkg.synthetic", "1.2.3")).expect("package finalizes");
    let digest = vocabulary_package_digest(&package).expect("digest computes");

    let left = validate_relation_fact(
        &package,
        &RelationFact {
            package_digest: digest.clone(),
            term_id: "pkg.synthetic:co_assignee".to_string(),
            subject_id: "person:alex".to_string(),
            subject_type_ref: "types.synthetic:person".to_string(),
            object_id: "person:blake".to_string(),
            object_type_ref: "types.synthetic:person".to_string(),
            interval: None,
        },
    )
    .expect("left relation validates");
    let right = validate_relation_fact(
        &package,
        &RelationFact {
            package_digest: digest,
            term_id: "pkg.synthetic:co_assignee".to_string(),
            subject_id: "person:blake".to_string(),
            subject_type_ref: "types.synthetic:person".to_string(),
            object_id: "person:alex".to_string(),
            object_type_ref: "types.synthetic:person".to_string(),
            interval: None,
        },
    )
    .expect("right relation validates");

    assert_eq!(left.relation.subject_id, "person:alex");
    assert_eq!(right.relation.subject_id, "person:blake");

    let serialized = serde_json::to_string(&(left, right)).expect("relations serialize");
    assert!(!serialized.contains("canonical_id"));
    assert!(!serialized.contains("same_as"));
}

#[test]
fn same_major_package_updates_are_compatible() {
    let locked = finalize_package(invented_package("pkg.synthetic", "1.2.3"))
        .expect("locked package finalizes");
    let locked_digest = vocabulary_package_digest(&locked).expect("digest computes");
    let reference = VocabularyTermRef {
        package_digest: locked_digest,
        term_id: "pkg.synthetic:assignee".to_string(),
    };

    let mut candidate = invented_package("pkg.synthetic", "1.4.0");
    candidate.terms[0].label = "Assignee v2 Label".to_string();
    let candidate = finalize_package(candidate).expect("candidate finalizes");

    assert_eq!(
        package_compatibility(&locked, &candidate, &[reference]).expect("same major compatible"),
        VocabularyPackageCompatibility::CompatibleSameMajor
    );
}

#[test]
fn canonical_package_bytes_are_stable_across_input_order() {
    let left = finalize_package(invented_package("pkg.synthetic", "1.2.3"))
        .expect("left package finalizes");
    let right = finalize_package(shuffled_invented_package("pkg.synthetic", "1.2.3"))
        .expect("right package finalizes");

    let left_bytes = canonical_package_bytes(&left).expect("left serializes");
    let right_bytes = canonical_package_bytes(&right).expect("right serializes");
    assert_eq!(left_bytes, right_bytes);
    assert_eq!(
        vocabulary_package_digest(&left).unwrap(),
        vocabulary_package_digest(&right).unwrap()
    );
}

#[test]
fn source_scan_keeps_domain_vocabulary_out_of_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "tranche", "servicer", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "vocabulary module should not embed domain term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "vocabulary schema should not embed domain term {banned}"
        );
    }
}

fn invented_package(package_id: &str, package_version: &str) -> VocabularyPackage {
    VocabularyPackage {
        version: String::new(),
        package_id: package_id.to_string(),
        package_version: package_version.to_string(),
        terms: vec![
            VocabularyTerm {
                term_id: format!("{package_id}:assignee"),
                kind: VocabularyTermKind::Role,
                label: "Assignee".to_string(),
                intake_names: vec!["desk lead".to_string(), "queue owner".to_string()],
                subject_type_refs: vec!["types.synthetic:person".to_string()],
                object_type_refs: vec!["types.synthetic:queue".to_string()],
                direction: RelationDirection::Directed,
                cardinality_hint: CardinalityHint::ManyToOne,
                interval_requirement: IntervalRequirement::Optional,
                identity_implication: false,
                documentation_refs: vec!["docs/vocabulary.md".to_string()],
            },
            VocabularyTerm {
                term_id: format!("{package_id}:co_assignee"),
                kind: VocabularyTermKind::Relationship,
                label: "Co-Assignee".to_string(),
                intake_names: vec!["backup assignee".to_string()],
                subject_type_refs: vec!["types.synthetic:person".to_string()],
                object_type_refs: vec!["types.synthetic:person".to_string()],
                direction: RelationDirection::Undirected,
                cardinality_hint: CardinalityHint::ManyToMany,
                interval_requirement: IntervalRequirement::Optional,
                identity_implication: false,
                documentation_refs: vec!["docs/vocabulary.md".to_string()],
            },
            VocabularyTerm {
                term_id: format!("{package_id}:coverage_pair"),
                kind: VocabularyTermKind::Relationship,
                label: "Coverage Pair".to_string(),
                intake_names: vec!["paired coverage".to_string()],
                subject_type_refs: vec!["types.synthetic:desk".to_string()],
                object_type_refs: vec!["types.synthetic:queue".to_string()],
                direction: RelationDirection::Undirected,
                cardinality_hint: CardinalityHint::ManyToMany,
                interval_requirement: IntervalRequirement::Optional,
                identity_implication: false,
                documentation_refs: vec!["docs/vocabulary.md".to_string()],
            },
            VocabularyTerm {
                term_id: format!("{package_id}:transfer"),
                kind: VocabularyTermKind::Relationship,
                label: "Transfer".to_string(),
                intake_names: vec!["handoff".to_string(), "reassignment".to_string()],
                subject_type_refs: vec!["types.synthetic:assignment".to_string()],
                object_type_refs: vec!["types.synthetic:assignment".to_string()],
                direction: RelationDirection::Directed,
                cardinality_hint: CardinalityHint::OneToMany,
                interval_requirement: IntervalRequirement::Required,
                identity_implication: false,
                documentation_refs: vec!["docs/vocabulary.md".to_string()],
            },
        ],
        documentation: vec![VocabularyDocumentationRef {
            label: "Vocabulary Guide".to_string(),
            uri: "docs/vocabulary.md".to_string(),
        }],
    }
}

fn ambiguous_package(package_id: &str, package_version: &str) -> VocabularyPackage {
    let mut package = invented_package(package_id, package_version);
    package.terms.push(VocabularyTerm {
        term_id: format!("{package_id}:desk_lead"),
        kind: VocabularyTermKind::Role,
        label: "Desk Lead".to_string(),
        intake_names: vec![],
        subject_type_refs: vec!["types.synthetic:person".to_string()],
        object_type_refs: vec!["types.synthetic:queue".to_string()],
        direction: RelationDirection::Directed,
        cardinality_hint: CardinalityHint::ManyToOne,
        interval_requirement: IntervalRequirement::Optional,
        identity_implication: false,
        documentation_refs: vec!["docs/vocabulary.md".to_string()],
    });
    package
}

fn shuffled_invented_package(package_id: &str, package_version: &str) -> VocabularyPackage {
    let mut package = invented_package(package_id, package_version);
    package.terms.reverse();
    package.terms[0].subject_type_refs.reverse();
    package.terms[0].object_type_refs.reverse();
    package.terms[1].intake_names.reverse();
    package
}
