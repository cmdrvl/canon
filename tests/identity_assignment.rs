#![forbid(unsafe_code)]

#[path = "../src/temporal/assignment.rs"]
mod assignment;

use assignment::{
    AssignmentAssignee, AssignmentConflictPolicy, AssignmentConstraints, AssignmentEntityTypeRef,
    AssignmentErrorCode, AssignmentPolicyRef, AssignmentProvenance, AssignmentRoleRef,
    AssignmentStatus, AssignmentStatusCode, AssignmentSubject, AssignmentSuccessorPolicy,
    CoreEntityTypeClass, IntervalBoundary, TimeInterval, TypedAssignmentFact,
    assignment_fact_implies_alias, assignment_projection_fields, assignment_schema_version,
    canonical_assignment_bytes, canonical_assignment_set_bytes, finalize_assignment,
    finalize_assignments,
};
use serde_json::Value;
use std::collections::BTreeMap;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.identity.assignment.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/temporal/assignment.rs");

#[test]
fn schema_declares_assignment_boundary_and_package_pins() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], "canon.identity.assignment.v1");
    assert_eq!(
        schema["properties"]["version"]["const"],
        "canon.identity.assignment.v1"
    );
    assert_eq!(
        schema["$defs"]["role_ref"]["properties"]["package_digest"]["$ref"],
        "#/$defs/blake3_hash"
    );
    assert_eq!(
        schema["$defs"]["role_ref"]["properties"]["term_id"]["$ref"],
        "#/$defs/opaque_role_term_id"
    );
    assert!(
        schema["x-canon-contract"]["identity_boundary"]
            .as_str()
            .unwrap()
            .contains("separate identity review")
    );
    assert_eq!(assignment_schema_version(), "canon.identity.assignment.v1");
}

#[test]
fn coassignments_and_transfers_stay_independent_from_aliases() {
    let co_left = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:alpha",
        assignee_id: "person:alex",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-06-30T23:59:59Z"),
        known_start: "2026-01-02T00:00:00Z",
        known_end: None,
        max_active: Some(2),
        conflict_policy: AssignmentConflictPolicy::Allow,
        successor_policy: AssignmentSuccessorPolicy::AllowParallel,
        status: AssignmentStatusCode::Asserted,
        reason_code: "co_assignment",
        supersedes: None,
        locator: "fixtures/assignment/co-left.jsonl",
    });
    let co_right = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:alpha",
        assignee_id: "person:blake",
        valid_start: "2026-03-01T00:00:00Z",
        valid_end: Some("2026-06-30T23:59:59Z"),
        known_start: "2026-03-02T00:00:00Z",
        known_end: None,
        max_active: Some(2),
        conflict_policy: AssignmentConflictPolicy::Allow,
        successor_policy: AssignmentSuccessorPolicy::AllowParallel,
        status: AssignmentStatusCode::Asserted,
        reason_code: "co_assignment",
        supersedes: None,
        locator: "fixtures/assignment/co-right.jsonl",
    });
    let transfer_out = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:beta",
        assignee_id: "person:alex",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-03-31T23:59:59Z"),
        known_start: "2026-01-02T00:00:00Z",
        known_end: None,
        max_active: Some(1),
        conflict_policy: AssignmentConflictPolicy::Disallow,
        successor_policy: AssignmentSuccessorPolicy::RequireNonOverlapping,
        status: AssignmentStatusCode::Asserted,
        reason_code: "transfer_out",
        supersedes: None,
        locator: "fixtures/assignment/transfer-out.jsonl",
    });
    let transfer_in = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:beta",
        assignee_id: "person:casey",
        valid_start: "2026-04-01T00:00:00Z",
        valid_end: None,
        known_start: "2026-04-02T00:00:00Z",
        known_end: None,
        max_active: Some(1),
        conflict_policy: AssignmentConflictPolicy::Disallow,
        successor_policy: AssignmentSuccessorPolicy::RequireNonOverlapping,
        status: AssignmentStatusCode::Asserted,
        reason_code: "transfer_in",
        supersedes: None,
        locator: "fixtures/assignment/transfer-in.jsonl",
    });

    let facts = finalize_assignments(vec![co_left, co_right, transfer_in, transfer_out])
        .expect("co-assignments and transfers validate");
    assert_eq!(facts.len(), 4);
    assert!(
        facts
            .iter()
            .all(|fact| !assignment_fact_implies_alias(fact))
    );
}

#[test]
fn resolved_identity_can_remain_assignment_disputed() {
    let left = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:gamma",
        assignee_id: "person:alex",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: None,
        known_start: "2026-01-03T00:00:00Z",
        known_end: None,
        max_active: Some(2),
        conflict_policy: AssignmentConflictPolicy::Review,
        successor_policy: AssignmentSuccessorPolicy::Review,
        status: AssignmentStatusCode::Disputed,
        reason_code: "source_disagreement",
        supersedes: None,
        locator: "fixtures/assignment/dispute-left.jsonl",
    });
    let right = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:gamma",
        assignee_id: "person:blake",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: None,
        known_start: "2026-01-04T00:00:00Z",
        known_end: None,
        max_active: Some(2),
        conflict_policy: AssignmentConflictPolicy::Review,
        successor_policy: AssignmentSuccessorPolicy::Review,
        status: AssignmentStatusCode::Disputed,
        reason_code: "source_disagreement",
        supersedes: None,
        locator: "fixtures/assignment/dispute-right.jsonl",
    });

    let facts =
        finalize_assignments(vec![left, right]).expect("reviewable disagreement must validate");
    assert!(
        facts
            .iter()
            .all(|fact| fact.status.code == AssignmentStatusCode::Disputed)
    );
}

#[test]
fn unresolved_assignee_retains_disclosed_value_and_source_locator() {
    let fact = observation_assignment();
    let finalized = finalize_assignment(fact).expect("observation assignment finalizes");
    let projection = assignment_projection_fields(&finalized).expect("projection builds");

    assert_eq!(projection["assignee_kind"], "unresolved_observation");
    assert_eq!(
        projection["assignee_disclosed_value"],
        "Operations Desk West"
    );
    assert_eq!(
        projection["source_locator"],
        "fixtures/assignment/unresolved-observation.jsonl"
    );
    assert_eq!(projection["raw.disclosed_role"], "desk_lead");
    assert!(!projection.contains_key("assignee_identity_id"));
}

#[test]
fn invalid_cardinality_refuses_parallel_exclusive_assignments() {
    let left = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:delta",
        assignee_id: "person:alex",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-06-30T23:59:59Z"),
        known_start: "2026-01-01T00:00:00Z",
        known_end: None,
        max_active: Some(1),
        conflict_policy: AssignmentConflictPolicy::Disallow,
        successor_policy: AssignmentSuccessorPolicy::RequireNonOverlapping,
        status: AssignmentStatusCode::Asserted,
        reason_code: "exclusive_role",
        supersedes: None,
        locator: "fixtures/assignment/exclusive-left.jsonl",
    });
    let right = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:delta",
        assignee_id: "person:blake",
        valid_start: "2026-03-01T00:00:00Z",
        valid_end: Some("2026-09-30T23:59:59Z"),
        known_start: "2026-03-01T00:00:00Z",
        known_end: None,
        max_active: Some(1),
        conflict_policy: AssignmentConflictPolicy::Disallow,
        successor_policy: AssignmentSuccessorPolicy::RequireNonOverlapping,
        status: AssignmentStatusCode::Asserted,
        reason_code: "exclusive_role",
        supersedes: None,
        locator: "fixtures/assignment/exclusive-right.jsonl",
    });

    let error = finalize_assignments(vec![left, right])
        .expect_err("exclusive overlapping assignments must fail");
    assert_eq!(error.code, AssignmentErrorCode::PolicyConstraint);
}

#[test]
fn late_correction_requires_superseded_assignment_reference() {
    let original = finalize_assignment(entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:epsilon",
        assignee_id: "person:alex",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: None,
        known_start: "2026-01-01T00:00:00Z",
        known_end: None,
        max_active: Some(1),
        conflict_policy: AssignmentConflictPolicy::Disallow,
        successor_policy: AssignmentSuccessorPolicy::RequireNonOverlapping,
        status: AssignmentStatusCode::Asserted,
        reason_code: "initial",
        supersedes: None,
        locator: "fixtures/assignment/original.jsonl",
    }))
    .expect("original assignment finalizes");

    let missing_ref = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:epsilon",
        assignee_id: "person:blake",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: None,
        known_start: "2026-05-01T00:00:00Z",
        known_end: None,
        max_active: Some(2),
        conflict_policy: AssignmentConflictPolicy::Review,
        successor_policy: AssignmentSuccessorPolicy::Review,
        status: AssignmentStatusCode::Corrected,
        reason_code: "late_correction",
        supersedes: None,
        locator: "fixtures/assignment/correction-missing-ref.jsonl",
    });
    let error = finalize_assignment(missing_ref).expect_err("missing supersedes ref must fail");
    assert_eq!(error.code, AssignmentErrorCode::PolicyConstraint);

    let corrected = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:epsilon",
        assignee_id: "person:blake",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: None,
        known_start: "2026-05-01T00:00:00Z",
        known_end: None,
        max_active: Some(2),
        conflict_policy: AssignmentConflictPolicy::Review,
        successor_policy: AssignmentSuccessorPolicy::Review,
        status: AssignmentStatusCode::Corrected,
        reason_code: "late_correction",
        supersedes: Some(original.assignment_id.clone()),
        locator: "fixtures/assignment/correction.jsonl",
    });
    let corrected = finalize_assignment(corrected).expect("correction finalizes");
    assert_eq!(
        corrected.status.supersedes_assignment_id.as_deref(),
        Some(original.assignment_id.as_str())
    );
}

#[test]
fn canonical_assignment_set_bytes_are_stable_across_input_order() {
    let left = observation_assignment();
    let right = entity_assignment(EntityAssignmentSpec {
        subject_id: "desk:zeta",
        assignee_id: "person:alex",
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: None,
        known_start: "2026-01-02T00:00:00Z",
        known_end: None,
        max_active: Some(2),
        conflict_policy: AssignmentConflictPolicy::Allow,
        successor_policy: AssignmentSuccessorPolicy::AllowParallel,
        status: AssignmentStatusCode::Asserted,
        reason_code: "ordered_bytes",
        supersedes: None,
        locator: "fixtures/assignment/entity.jsonl",
    });

    let bytes_a =
        canonical_assignment_set_bytes(&[left.clone(), right.clone()]).expect("left bytes");
    let bytes_b = canonical_assignment_set_bytes(&[right, left]).expect("right bytes");
    assert_eq!(bytes_a, bytes_b);

    let single = canonical_assignment_bytes(&observation_assignment()).expect("single bytes");
    assert!(!single.is_empty());
}

#[test]
fn source_scan_keeps_domain_vocabulary_out_of_assignment_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "tranche", "servicer", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "assignment module should not embed domain term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "assignment schema should not embed domain term {banned}"
        );
    }
}

struct EntityAssignmentSpec<'a> {
    subject_id: &'a str,
    assignee_id: &'a str,
    valid_start: &'a str,
    valid_end: Option<&'a str>,
    known_start: &'a str,
    known_end: Option<&'a str>,
    max_active: Option<u32>,
    conflict_policy: AssignmentConflictPolicy,
    successor_policy: AssignmentSuccessorPolicy,
    status: AssignmentStatusCode,
    reason_code: &'a str,
    supersedes: Option<String>,
    locator: &'a str,
}

fn entity_assignment(spec: EntityAssignmentSpec<'_>) -> TypedAssignmentFact {
    TypedAssignmentFact {
        version: String::new(),
        assignment_id: String::new(),
        assignment_key: String::new(),
        subject: AssignmentSubject {
            identity_id: spec.subject_id.to_string(),
            entity_type: AssignmentEntityTypeRef::Core {
                class: CoreEntityTypeClass::Organization,
            },
        },
        role: sample_role_ref(),
        assignee: AssignmentAssignee::Entity {
            identity_id: spec.assignee_id.to_string(),
            entity_type: AssignmentEntityTypeRef::Core {
                class: CoreEntityTypeClass::Person,
            },
        },
        valid_time: interval(spec.valid_start, spec.valid_end),
        known_time: interval(spec.known_start, spec.known_end),
        policy_ref: sample_policy_ref(),
        constraints: AssignmentConstraints {
            max_active_assignees_per_subject_role: spec.max_active,
            allow_unresolved_assignee: false,
            conflict_policy: spec.conflict_policy,
            successor_policy: spec.successor_policy,
        },
        status: AssignmentStatus {
            code: spec.status,
            reason_code: spec.reason_code.to_string(),
            supersedes_assignment_id: spec.supersedes,
        },
        provenance: AssignmentProvenance {
            source_system: "fixture_catalog".to_string(),
            locator: spec.locator.to_string(),
            fragment: Some("row-1".to_string()),
            observed_at: Some("2026-08-01T09:30:00Z".to_string()),
            raw_fields: BTreeMap::from([(
                "reported_assignee".to_string(),
                spec.assignee_id.to_string(),
            )]),
        },
    }
}

fn observation_assignment() -> TypedAssignmentFact {
    TypedAssignmentFact {
        version: String::new(),
        assignment_id: String::new(),
        assignment_key: String::new(),
        subject: AssignmentSubject {
            identity_id: "desk:west".to_string(),
            entity_type: AssignmentEntityTypeRef::Core {
                class: CoreEntityTypeClass::Organization,
            },
        },
        role: sample_role_ref(),
        assignee: AssignmentAssignee::Observation {
            disclosed_value: "Operations Desk West".to_string(),
            entity_type: AssignmentEntityTypeRef::Extension {
                package_digest: sample_hash('b'),
                vocabulary: "entity_type".to_string(),
                value: "team".to_string(),
            },
        },
        valid_time: interval("2026-01-01T00:00:00Z", None),
        known_time: interval("2026-01-05T00:00:00Z", None),
        policy_ref: sample_policy_ref(),
        constraints: AssignmentConstraints {
            max_active_assignees_per_subject_role: Some(3),
            allow_unresolved_assignee: true,
            conflict_policy: AssignmentConflictPolicy::Review,
            successor_policy: AssignmentSuccessorPolicy::Review,
        },
        status: AssignmentStatus {
            code: AssignmentStatusCode::Disputed,
            reason_code: "unresolved_disclosed_observation".to_string(),
            supersedes_assignment_id: None,
        },
        provenance: AssignmentProvenance {
            source_system: "fixture_catalog".to_string(),
            locator: "fixtures/assignment/unresolved-observation.jsonl".to_string(),
            fragment: Some("row-7".to_string()),
            observed_at: Some("2026-08-01T09:30:00Z".to_string()),
            raw_fields: BTreeMap::from([
                ("disclosed_role".to_string(), "desk_lead".to_string()),
                (
                    "reported_assignee".to_string(),
                    "Operations Desk West".to_string(),
                ),
            ]),
        },
    }
}

fn interval(start_at: &str, end_at: Option<&str>) -> TimeInterval {
    TimeInterval {
        start_at: Some(start_at.to_string()),
        start_bound: IntervalBoundary::Inclusive,
        end_at: end_at.map(ToString::to_string),
        end_bound: if end_at.is_some() {
            IntervalBoundary::Inclusive
        } else {
            IntervalBoundary::Open
        },
    }
}

fn sample_role_ref() -> AssignmentRoleRef {
    AssignmentRoleRef {
        package_digest: sample_hash('a'),
        term_id: "pkg.synthetic:assignee".to_string(),
    }
}

fn sample_policy_ref() -> AssignmentPolicyRef {
    AssignmentPolicyRef {
        package_digest: sample_hash('c'),
        policy_id: "assignment.default.v1".to_string(),
    }
}

fn sample_hash(hex: char) -> String {
    format!(
        "blake3:{}",
        std::iter::repeat_n(hex, 64).collect::<String>()
    )
}
