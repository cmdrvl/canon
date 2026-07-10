#![forbid(unsafe_code)]

#[path = "../src/temporal/relation.rs"]
mod relation;

use relation::{
    CoreEntityTypeClass, CoreRelationClass, DirectedRelationFact, IntervalBoundary,
    RelationCardinalityConstraints, RelationCyclePolicy, RelationEntityTypeRef, RelationErrorCode,
    RelationIdentityImplication, RelationIdentityImplicationMode, RelationKindRef,
    RelationOverlapPolicy, RelationProvenance, RelationReview, ReviewDisposition, TimeInterval,
    canonical_relation_bytes, canonical_relation_set_bytes, finalize_relation, finalize_relations,
    relation_edge_implies_alias, relation_schema_version, relation_timeline_for_subject,
    review_concepts_are_distinct,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.identity.relation.v1.schema.json");

#[test]
fn schema_declares_relation_edges_are_not_aliases() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], "canon.identity.relation.v1");
    assert_eq!(
        schema["properties"]["version"]["const"],
        "canon.identity.relation.v1"
    );
    assert_eq!(
        schema["x-canon-contract"]["identity_implication_default"],
        "none"
    );
    assert_eq!(
        schema["x-canon-contract"]["review_concepts"],
        serde_json::json!(["same", "distinct", "related", "uncertain"])
    );
    assert!(
        schema["x-canon-contract"]["same_as_boundary"]
            .as_str()
            .unwrap()
            .contains("separate identity fact")
    );
    assert_eq!(relation_schema_version(), "canon.identity.relation.v1");
}

#[test]
fn relation_edges_never_become_aliases_solely_from_connectivity() {
    let parent_child = finalize_relation(sample_relation(SampleRelationSpec {
        subject_id: "org:parent",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Hierarchy,
        object_id: "org:child",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-01-01T00:00:00Z",
        end_at: Some("2026-12-31T23:59:59Z"),
        review: sample_review(ReviewDisposition::Related, "parent_child"),
    }))
    .expect("parent-child finalizes");

    let same_review = finalize_relation(DirectedRelationFact {
        review: RelationReview {
            disposition: ReviewDisposition::Same,
            reason_code: "review_same_pending_equality_fact".to_string(),
        },
        identity_implication: RelationIdentityImplication::default(),
        ..sample_relation(SampleRelationSpec {
            subject_id: "org:brand",
            subject_type: CoreEntityTypeClass::Organization,
            relation_class: CoreRelationClass::Association,
            object_id: "org:operator",
            object_type: CoreEntityTypeClass::Organization,
            start_at: "2026-01-01T00:00:00Z",
            end_at: None,
            review: sample_review(ReviewDisposition::Same, "ignored"),
        })
    })
    .expect("same review finalizes");

    assert!(!relation_edge_implies_alias(&parent_child));
    assert!(!relation_edge_implies_alias(&same_review));
    assert_eq!(
        same_review.identity_implication.mode,
        RelationIdentityImplicationMode::None
    );
}

#[test]
fn timeline_queries_keep_provenance_and_intervals_for_hierarchy_role_and_successor_edges() {
    let parent = sample_relation(SampleRelationSpec {
        subject_id: "org:parent",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Hierarchy,
        object_id: "org:child",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2025-01-01T00:00:00Z",
        end_at: None,
        review: sample_review(ReviewDisposition::Related, "parent_child"),
    });
    let role = sample_relation(SampleRelationSpec {
        subject_id: "person:assignee_a",
        subject_type: CoreEntityTypeClass::Person,
        relation_class: CoreRelationClass::Role,
        object_id: "org:desk_a",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-01-01T00:00:00Z",
        end_at: Some("2026-06-30T23:59:59Z"),
        review: sample_review(ReviewDisposition::Related, "assignment"),
    });
    let successor = sample_relation(SampleRelationSpec {
        subject_id: "org:legacy",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Succession,
        object_id: "org:successor",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-07-01T00:00:00Z",
        end_at: None,
        review: sample_review(ReviewDisposition::Related, "successor"),
    });

    let timeline = relation_timeline_for_subject(
        &[successor.clone(), role.clone(), parent.clone()],
        "org:parent",
    )
    .expect("timeline query works");
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].provenance.source_system, "fixture_catalog");
    assert_eq!(
        timeline[0].valid_time.start_at.as_deref(),
        Some("2025-01-01T00:00:00Z")
    );

    let assignee_timeline =
        relation_timeline_for_subject(&[successor, role], "person:assignee_a").expect("role query");
    assert_eq!(assignee_timeline.len(), 1);
    assert_eq!(
        assignee_timeline[0].valid_time.end_at.as_deref(),
        Some("2026-06-30T23:59:59Z")
    );
}

#[test]
fn cycle_and_overlapping_exclusive_constraints_are_policy_driven_and_reviewable() {
    let cycle_a = sample_relation(SampleRelationSpec {
        subject_id: "org:a",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Hierarchy,
        object_id: "org:b",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-01-01T00:00:00Z",
        end_at: None,
        review: sample_review(ReviewDisposition::Related, "parent"),
    });
    let cycle_b = sample_relation(SampleRelationSpec {
        subject_id: "org:b",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Hierarchy,
        object_id: "org:c",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-01-01T00:00:00Z",
        end_at: None,
        review: sample_review(ReviewDisposition::Related, "parent"),
    });
    let cycle_c = sample_relation(SampleRelationSpec {
        subject_id: "org:c",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Hierarchy,
        object_id: "org:a",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-01-01T00:00:00Z",
        end_at: None,
        review: sample_review(ReviewDisposition::Related, "parent"),
    });
    let error = finalize_relations(vec![cycle_a, cycle_b, cycle_c]).expect_err("cycle refuses");
    assert_eq!(error.code, RelationErrorCode::PolicyConstraint);

    let left = role_assignment(
        "person:desk_owner",
        "org:desk_a",
        "2026-01-01T00:00:00Z",
        Some("2026-06-30T23:59:59Z"),
    );
    let right = role_assignment(
        "person:desk_owner",
        "org:desk_b",
        "2026-03-01T00:00:00Z",
        Some("2026-12-31T23:59:59Z"),
    );
    let error = finalize_relations(vec![left, right]).expect_err("overlapping exclusive refuses");
    assert_eq!(error.code, RelationErrorCode::PolicyConstraint);

    let review_allowed = review_cycle_relation("org:a", "org:b");
    let review_allowed_back = review_cycle_relation("org:b", "org:a");
    finalize_relations(vec![review_allowed, review_allowed_back])
        .expect("review policy allows cycle");
}

#[test]
fn same_distinct_related_and_uncertain_reviews_stay_separate_and_extension_vocabularies_are_pinned()
{
    let same = finalize_relation(extension_relation(
        ReviewDisposition::Same,
        "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ))
    .expect("same review finalizes");
    let distinct = finalize_relation(extension_relation(
        ReviewDisposition::Distinct,
        "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ))
    .expect("distinct review finalizes");
    let related = finalize_relation(extension_relation(
        ReviewDisposition::Related,
        "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ))
    .expect("related review finalizes");
    let uncertain = finalize_relation(extension_relation(
        ReviewDisposition::Uncertain,
        "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ))
    .expect("uncertain review finalizes");

    assert!(review_concepts_are_distinct(&same.review, &distinct.review));
    assert!(review_concepts_are_distinct(
        &distinct.review,
        &related.review
    ));
    assert!(review_concepts_are_distinct(
        &related.review,
        &uncertain.review
    ));

    let bad_extension =
        finalize_relation(extension_relation(ReviewDisposition::Related, "not-a-hash"))
            .expect_err("bad extension pin refuses");
    assert_eq!(bad_extension.code, RelationErrorCode::CorruptReference);
}

#[test]
fn canonical_relation_set_bytes_are_stable_across_input_order() {
    let first = sample_relation(SampleRelationSpec {
        subject_id: "org:brand",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Association,
        object_id: "org:operator",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-01-01T00:00:00Z",
        end_at: None,
        review: sample_review(ReviewDisposition::Related, "brand_family"),
    });
    let second = sample_relation(SampleRelationSpec {
        subject_id: "org:legacy",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Succession,
        object_id: "org:successor",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-07-01T00:00:00Z",
        end_at: None,
        review: sample_review(ReviewDisposition::Related, "successor"),
    });

    let left = canonical_relation_set_bytes(&[second.clone(), first.clone()]).expect("left bytes");
    let right = canonical_relation_set_bytes(&[first, second]).expect("right bytes");
    assert_eq!(left, right);

    let single = canonical_relation_bytes(&sample_relation(SampleRelationSpec {
        subject_id: "org:parent",
        subject_type: CoreEntityTypeClass::Organization,
        relation_class: CoreRelationClass::Hierarchy,
        object_id: "org:child",
        object_type: CoreEntityTypeClass::Organization,
        start_at: "2026-01-01T00:00:00Z",
        end_at: None,
        review: sample_review(ReviewDisposition::Related, "parent_child"),
    }))
    .expect("single relation bytes");
    assert!(!single.is_empty());
}

struct SampleRelationSpec<'a> {
    subject_id: &'a str,
    subject_type: CoreEntityTypeClass,
    relation_class: CoreRelationClass,
    object_id: &'a str,
    object_type: CoreEntityTypeClass,
    start_at: &'a str,
    end_at: Option<&'a str>,
    review: RelationReview,
}

fn sample_relation(spec: SampleRelationSpec<'_>) -> DirectedRelationFact {
    DirectedRelationFact {
        version: String::new(),
        relation_id: String::new(),
        edge_key: String::new(),
        subject: relation::RelationEndpoint {
            identity_id: spec.subject_id.to_string(),
            entity_type: RelationEntityTypeRef::Core {
                class: spec.subject_type,
            },
        },
        relation: RelationKindRef::Core {
            class: spec.relation_class,
        },
        object: relation::RelationEndpoint {
            identity_id: spec.object_id.to_string(),
            entity_type: RelationEntityTypeRef::Core {
                class: spec.object_type,
            },
        },
        valid_time: TimeInterval {
            start_at: Some(spec.start_at.to_string()),
            start_bound: IntervalBoundary::Inclusive,
            end_at: spec.end_at.map(ToString::to_string),
            end_bound: if spec.end_at.is_some() {
                IntervalBoundary::Inclusive
            } else {
                IntervalBoundary::Open
            },
        },
        provenance: RelationProvenance {
            source_system: "fixture_catalog".to_string(),
            locator: "fixtures/temporal/relation.jsonl".to_string(),
            fragment: Some("row-1".to_string()),
            observed_at: Some("2026-08-01T09:30:00Z".to_string()),
        },
        policy_ref: "relation.default.v1".to_string(),
        constraints: RelationCardinalityConstraints {
            max_objects_per_subject: None,
            max_subjects_per_object: None,
            overlap_policy: RelationOverlapPolicy::Allow,
            cycle_policy: RelationCyclePolicy::Disallow,
        },
        identity_implication: RelationIdentityImplication::default(),
        review: spec.review,
    }
}

fn role_assignment(
    subject_id: &str,
    object_id: &str,
    start_at: &str,
    end_at: Option<&str>,
) -> DirectedRelationFact {
    DirectedRelationFact {
        constraints: RelationCardinalityConstraints {
            max_objects_per_subject: Some(1),
            max_subjects_per_object: None,
            overlap_policy: RelationOverlapPolicy::Disallow,
            cycle_policy: RelationCyclePolicy::Allow,
        },
        policy_ref: "role.exclusive.v1".to_string(),
        review: sample_review(ReviewDisposition::Related, "role_assignment"),
        ..sample_relation(SampleRelationSpec {
            subject_id,
            subject_type: CoreEntityTypeClass::Person,
            relation_class: CoreRelationClass::Role,
            object_id,
            object_type: CoreEntityTypeClass::Organization,
            start_at,
            end_at,
            review: sample_review(ReviewDisposition::Related, "role_assignment"),
        })
    }
}

fn review_cycle_relation(subject_id: &str, object_id: &str) -> DirectedRelationFact {
    DirectedRelationFact {
        constraints: RelationCardinalityConstraints {
            max_objects_per_subject: None,
            max_subjects_per_object: None,
            overlap_policy: RelationOverlapPolicy::Allow,
            cycle_policy: RelationCyclePolicy::Review,
        },
        policy_ref: "hierarchy.reviewable.v1".to_string(),
        ..sample_relation(SampleRelationSpec {
            subject_id,
            subject_type: CoreEntityTypeClass::Organization,
            relation_class: CoreRelationClass::Hierarchy,
            object_id,
            object_type: CoreEntityTypeClass::Organization,
            start_at: "2026-01-01T00:00:00Z",
            end_at: None,
            review: sample_review(ReviewDisposition::Related, "review_cycle"),
        })
    }
}

fn extension_relation(
    disposition: ReviewDisposition,
    package_digest: &str,
) -> DirectedRelationFact {
    DirectedRelationFact {
        relation: RelationKindRef::Extension {
            package_digest: package_digest.to_string(),
            vocabulary: "ontology".to_string(),
            value: "brand_family".to_string(),
        },
        review: RelationReview {
            disposition,
            reason_code: "review_state".to_string(),
        },
        ..sample_relation(SampleRelationSpec {
            subject_id: "org:left",
            subject_type: CoreEntityTypeClass::Organization,
            relation_class: CoreRelationClass::Association,
            object_id: "org:right",
            object_type: CoreEntityTypeClass::Organization,
            start_at: "2026-01-01T00:00:00Z",
            end_at: None,
            review: sample_review(disposition, "review_state"),
        })
    }
}

fn sample_review(disposition: ReviewDisposition, reason_code: &str) -> RelationReview {
    RelationReview {
        disposition,
        reason_code: reason_code.to_string(),
    }
}
