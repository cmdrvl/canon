#![forbid(unsafe_code)]

use canon::temporal::diff::{
    CANON_TEMPORAL_DIFF_VERSION, TemporalDiffFilter, TemporalDiffPageRequest, TemporalDiffRequest,
    canonical_diff_bytes, diff_temporal_snapshots,
};
use canon::temporal::explain::{
    CANON_TEMPORAL_EXPLAIN_VERSION, TemporalChangeClass, TemporalExactResult,
    TemporalExplainRequest, TemporalExplainSubject, TemporalIdentitySnapshot,
    TemporalRelationshipRef, TemporalSnapshotReference, canonical_explain_bytes,
    explain_temporal_identity,
};
use canon::temporal::{
    AssertionStatus, FactScope, IdentityFact, IntervalBoundary, RecordedTime, SourceLocator,
    TimeInterval, finalize_fact,
};

#[test]
fn no_change_explanation_names_snapshot_and_supporting_fact_set() {
    let fact = accepted_fact(
        "alias:acme",
        "person:alpha",
        "feed_a",
        "2026-01-02T00:00:00Z",
    );
    let snapshot = snapshot(
        "snap-a",
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
        vec![fact],
    );
    let artifact = explain_temporal_identity(TemporalExplainRequest {
        version: CANON_TEMPORAL_EXPLAIN_VERSION.to_string(),
        subject: TemporalExplainSubject::Surface {
            subject_id: "alias:acme".to_string(),
        },
        snapshots: vec![snapshot],
        max_chain_facts: None,
    })
    .expect("explanation builds");

    assert_eq!(artifact.summary.snapshot_count, 1);
    assert_eq!(artifact.summary.mapped_snapshot_count, 1);
    assert_eq!(artifact.snapshots[0].snapshot.snapshot_id, "snap-a");
    assert_eq!(artifact.snapshots[0].causal_chain.len(), 1);
    assert_eq!(
        artifact.timeline[0].change_class,
        TemporalChangeClass::NewFact
    );
    match &artifact.snapshots[0].exact_result {
        TemporalExactResult::SurfaceMapping {
            canonical_id,
            canonical_type,
            ..
        } => {
            assert_eq!(canonical_id, "person:alpha");
            assert_eq!(canonical_type, "person");
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let bytes_a = canonical_explain_bytes(&artifact).expect("canonical bytes");
    let round_tripped = serde_json::from_slice(&bytes_a).expect("artifact round trips");
    let bytes_b = canonical_explain_bytes(&round_tripped).expect("second canonical bytes");
    assert_eq!(bytes_a, bytes_b);
}

#[test]
fn unchanged_diff_names_compiled_snapshots_and_supporting_fact_set() {
    let fact = finalize_fact(accepted_fact(
        "alias:acme",
        "person:alpha",
        "feed_a",
        "2026-01-02T00:00:00Z",
    ))
    .expect("fact finalizes");
    let before = snapshot(
        "snap-unchanged-before",
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
        vec![fact.clone()],
    );
    let after = snapshot(
        "snap-unchanged-after",
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
        vec![fact.clone()],
    );

    let diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before,
        after,
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest::default(),
        include_unchanged: true,
    })
    .expect("unchanged diff builds");

    assert_eq!(diff.before_snapshot.snapshot_id, "snap-unchanged-before");
    assert_eq!(diff.after_snapshot.snapshot_id, "snap-unchanged-after");
    assert_eq!(diff.summary.compared_subject_count, 1);
    assert_eq!(diff.summary.changed_subject_count, 1);
    assert_eq!(diff.changes.len(), 1);
    let change = &diff.changes[0];
    assert_eq!(change.change_class, TemporalChangeClass::NoChange);
    assert_eq!(change.subject_id, "alias:acme");
    assert_eq!(change.causal_chain.len(), 1);
    assert_eq!(change.causal_chain[0].fact_id, fact.fact_id);
    match (&change.before, &change.after) {
        (
            TemporalExactResult::SurfaceMapping {
                canonical_id: before_id,
                fact_ids: before_facts,
                ..
            },
            TemporalExactResult::SurfaceMapping {
                canonical_id: after_id,
                fact_ids: after_facts,
                ..
            },
        ) => {
            assert_eq!(before_id, "person:alpha");
            assert_eq!(after_id, "person:alpha");
            assert_eq!(before_facts, after_facts);
        }
        other => panic!("unexpected unchanged result: {other:?}"),
    }
}

#[test]
fn known_time_correction_is_distinct_from_valid_time_expiry() {
    let original = finalize_fact(accepted_fact(
        "alias:acme",
        "person:alpha",
        "feed_a",
        "2026-01-02T00:00:00Z",
    ))
    .expect("original finalizes");
    let correction = accepted_fact_with_links(
        "alias:acme",
        "person:beta",
        "feed_a",
        "2026-02-02T00:00:00Z",
        vec![original.fact_id.clone()],
        Vec::new(),
        AssertionStatus::Accepted,
    );

    let before = snapshot(
        "snap-before",
        "2026-03-01T00:00:00Z",
        "2026-01-15T00:00:00Z",
        vec![original.clone(), correction.clone()],
    );
    let after_known = snapshot(
        "snap-after-known",
        "2026-03-01T00:00:00Z",
        "2026-03-15T00:00:00Z",
        vec![original.clone(), correction],
    );
    let correction_diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before,
        after: after_known,
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest {
            limit: 10,
            after_cursor: None,
        },
        include_unchanged: false,
    })
    .expect("known-time diff builds");
    assert_eq!(correction_diff.changes.len(), 1);
    assert_eq!(
        correction_diff.changes[0].change_class,
        TemporalChangeClass::Correction
    );
    assert_eq!(
        correction_diff.before_snapshot.valid_at,
        correction_diff.after_snapshot.valid_at
    );
    assert_ne!(
        correction_diff.before_snapshot.known_as_of,
        correction_diff.after_snapshot.known_as_of
    );

    let before_valid = snapshot(
        "snap-valid-before",
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
        vec![original.clone()],
    );
    let after_valid = snapshot(
        "snap-valid-after",
        "2027-01-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
        vec![original],
    );
    let expiry_diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: before_valid,
        after: after_valid,
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest::default(),
        include_unchanged: false,
    })
    .expect("valid-time diff builds");
    assert_eq!(expiry_diff.changes.len(), 1);
    assert_eq!(
        expiry_diff.changes[0].change_class,
        TemporalChangeClass::ExpiredFact
    );
    assert_ne!(
        expiry_diff.before_snapshot.valid_at,
        expiry_diff.after_snapshot.valid_at
    );
    assert_eq!(
        expiry_diff.before_snapshot.known_as_of,
        expiry_diff.after_snapshot.known_as_of
    );
}

#[test]
fn retractions_conflicts_policy_and_scope_changes_are_classified() {
    let original = finalize_fact(accepted_fact(
        "alias:acme",
        "person:alpha",
        "feed_a",
        "2026-01-02T00:00:00Z",
    ))
    .expect("original finalizes");
    let retraction = accepted_fact_with_links(
        "alias:acme",
        "person:alpha",
        "feed_a",
        "2026-04-01T00:00:00Z",
        Vec::new(),
        vec![original.fact_id.clone()],
        AssertionStatus::Retracted,
    );
    let retraction_diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: snapshot(
            "snap-ret-before",
            "2026-03-01T00:00:00Z",
            "2026-03-01T00:00:00Z",
            vec![original.clone(), retraction.clone()],
        ),
        after: snapshot(
            "snap-ret-after",
            "2026-03-01T00:00:00Z",
            "2026-05-01T00:00:00Z",
            vec![original.clone(), retraction],
        ),
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest::default(),
        include_unchanged: false,
    })
    .expect("retraction diff builds");
    assert_eq!(
        retraction_diff.changes[0].change_class,
        TemporalChangeClass::Retraction
    );

    let conflict_diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: snapshot(
            "snap-conf-before",
            "2026-03-01T00:00:00Z",
            "2026-03-01T00:00:00Z",
            vec![original.clone()],
        ),
        after: snapshot(
            "snap-conf-after",
            "2026-03-01T00:00:00Z",
            "2026-03-01T00:00:00Z",
            vec![
                original.clone(),
                accepted_fact(
                    "alias:acme",
                    "person:beta",
                    "feed_b",
                    "2026-01-03T00:00:00Z",
                ),
            ],
        ),
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest::default(),
        include_unchanged: false,
    })
    .expect("conflict diff builds");
    assert_eq!(
        conflict_diff.changes[0].change_class,
        TemporalChangeClass::Conflict
    );

    let policy_diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: snapshot_with_policy(
            "snap-policy-before",
            "2026-03-01T00:00:00Z",
            "2026-03-01T00:00:00Z",
            "policy.default",
            "1",
            vec![original.clone()],
        ),
        after: snapshot_with_policy(
            "snap-policy-after",
            "2026-03-01T00:00:00Z",
            "2026-03-01T00:00:00Z",
            "policy.default",
            "2",
            vec![original.clone()],
        ),
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest::default(),
        include_unchanged: false,
    })
    .expect("policy diff builds");
    assert_eq!(
        policy_diff.changes[0].change_class,
        TemporalChangeClass::PolicyChange
    );

    let scoped = IdentityFact {
        scope: Some(FactScope {
            scope_type: "tenant".to_string(),
            scope_id: "book-b".to_string(),
        }),
        materialization_digest: sample_hash('f'),
        ..accepted_fact(
            "alias:acme",
            "person:alpha",
            "feed_a",
            "2026-01-02T00:00:00Z",
        )
    };
    let scope_diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: snapshot(
            "snap-scope-before",
            "2026-03-01T00:00:00Z",
            "2026-03-01T00:00:00Z",
            vec![original],
        ),
        after: snapshot(
            "snap-scope-after",
            "2026-03-01T00:00:00Z",
            "2026-03-01T00:00:00Z",
            vec![scoped],
        ),
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest::default(),
        include_unchanged: false,
    })
    .expect("scope diff builds");
    assert_eq!(
        scope_diff.changes[0].change_class,
        TemporalChangeClass::ScopeChange
    );
}

#[test]
fn diff_pagination_filters_and_canonical_bytes_are_deterministic() {
    let before = snapshot(
        "snap-page-before",
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
        vec![accepted_fact(
            "alias:a",
            "person:alpha",
            "feed_a",
            "2026-01-02T00:00:00Z",
        )],
    );
    let after = snapshot(
        "snap-page-after",
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
        vec![
            accepted_fact("alias:a", "person:alpha", "feed_a", "2026-01-02T00:00:00Z"),
            accepted_fact("alias:b", "person:beta", "feed_b", "2026-01-02T00:00:00Z"),
            accepted_fact("alias:c", "org:gamma", "feed_b", "2026-01-02T00:00:00Z"),
        ],
    );
    let first_page = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: before.clone(),
        after: after.clone(),
        filter: TemporalDiffFilter {
            entity_types: vec!["person".to_string()],
            scopes: Vec::new(),
            source_systems: vec!["feed_b".to_string()],
            change_classes: vec![TemporalChangeClass::NewFact],
        },
        page: TemporalDiffPageRequest {
            limit: 1,
            after_cursor: None,
        },
        include_unchanged: false,
    })
    .expect("first page builds");
    assert_eq!(first_page.page.total_matching, 1);
    assert_eq!(first_page.changes[0].subject_id, "alias:b");
    assert_eq!(first_page.page.next_cursor, None);

    let all_changes = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before,
        after,
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest {
            limit: 1,
            after_cursor: None,
        },
        include_unchanged: false,
    })
    .expect("all changes builds");
    assert_eq!(all_changes.page.total_matching, 2);
    assert!(all_changes.page.next_cursor.is_some());
    let bytes_a = canonical_diff_bytes(&all_changes).expect("canonical diff bytes");
    let round_tripped = serde_json::from_slice(&bytes_a).expect("diff round trips");
    let bytes_b = canonical_diff_bytes(&round_tripped).expect("second diff bytes");
    assert_eq!(bytes_a, bytes_b);
}

#[test]
fn relationship_context_is_explanatory_not_equivalence_evidence() {
    let fact = accepted_fact(
        "alias:acme",
        "person:alpha",
        "feed_a",
        "2026-01-02T00:00:00Z",
    );
    let mut snapshot = snapshot(
        "snap-rel",
        "2026-03-01T00:00:00Z",
        "2026-03-01T00:00:00Z",
        vec![fact],
    );
    snapshot.relationships.push(TemporalRelationshipRef {
        relationship_id: "rel:1".to_string(),
        subject_id: "person:alpha".to_string(),
        predicate: "member_of".to_string(),
        object_id: "org:board".to_string(),
        valid_time: interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"),
        recorded_time: recorded_time("2026-01-02T00:00:00Z", 99),
        source_locator: source("feed_a"),
        materialization_digest: sample_hash('9'),
        scope: None,
    });

    let artifact = explain_temporal_identity(TemporalExplainRequest {
        version: CANON_TEMPORAL_EXPLAIN_VERSION.to_string(),
        subject: TemporalExplainSubject::CanonicalEntity {
            canonical_id: "person:alpha".to_string(),
        },
        snapshots: vec![snapshot],
        max_chain_facts: None,
    })
    .expect("relationship explanation builds");

    assert_eq!(artifact.snapshots[0].relationships.len(), 1);
    assert_eq!(artifact.snapshots[0].causal_chain.len(), 1);
    match &artifact.snapshots[0].exact_result {
        TemporalExactResult::EntitySupport { subject_ids, .. } => {
            assert_eq!(subject_ids, &vec!["alias:acme".to_string()]);
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

fn snapshot(
    snapshot_id: &str,
    valid_at: &str,
    known_as_of: &str,
    facts: Vec<IdentityFact>,
) -> TemporalIdentitySnapshot {
    snapshot_with_policy(
        snapshot_id,
        valid_at,
        known_as_of,
        "policy.default",
        "1",
        facts,
    )
}

fn snapshot_with_policy(
    snapshot_id: &str,
    valid_at: &str,
    known_as_of: &str,
    policy_ref: &str,
    policy_version: &str,
    facts: Vec<IdentityFact>,
) -> TemporalIdentitySnapshot {
    TemporalIdentitySnapshot {
        snapshot: TemporalSnapshotReference {
            snapshot_id: snapshot_id.to_string(),
            registry_id: "temporal-registry".to_string(),
            registry_version: "1.0.0".to_string(),
            compiled_snapshot_digest: sample_hash(snapshot_id.chars().last().unwrap_or('a')),
            valid_at: valid_at.to_string(),
            known_as_of: known_as_of.to_string(),
            policy_ref: policy_ref.to_string(),
            policy_version: policy_version.to_string(),
        },
        facts,
        relationships: Vec::new(),
    }
}

fn accepted_fact(
    subject_id: &str,
    object_id: &str,
    source_system: &str,
    recorded_at: &str,
) -> IdentityFact {
    accepted_fact_with_links(
        subject_id,
        object_id,
        source_system,
        recorded_at,
        Vec::new(),
        Vec::new(),
        AssertionStatus::Accepted,
    )
}

fn accepted_fact_with_links(
    subject_id: &str,
    object_id: &str,
    source_system: &str,
    recorded_at: &str,
    supersedes: Vec<String>,
    retracts: Vec<String>,
    assertion_status: AssertionStatus,
) -> IdentityFact {
    IdentityFact {
        version: String::new(),
        fact_id: String::new(),
        assertion_key: String::new(),
        conflict_key: String::new(),
        subject_id: subject_id.to_string(),
        predicate: "same_as".to_string(),
        object_id: object_id.to_string(),
        valid_time: interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"),
        recorded_time: recorded_time(recorded_at, 1),
        source_locator: source(source_system),
        materialization_digest: sample_hash(source_system.chars().last().unwrap_or('a')),
        assertion_status,
        trust_policy_ref: "trust.default.v1".to_string(),
        scope: Some(FactScope {
            scope_type: "tenant".to_string(),
            scope_id: "book-a".to_string(),
        }),
        supersedes,
        retracts,
    }
}

fn interval(start_at: &str, end_at: &str) -> TimeInterval {
    TimeInterval {
        start_at: Some(start_at.to_string()),
        start_bound: IntervalBoundary::Inclusive,
        end_at: Some(end_at.to_string()),
        end_bound: IntervalBoundary::Inclusive,
    }
}

fn recorded_time(start_at: &str, transaction_seq: u64) -> RecordedTime {
    RecordedTime {
        start_at: Some(start_at.to_string()),
        start_bound: IntervalBoundary::Inclusive,
        end_at: None,
        end_bound: IntervalBoundary::Open,
        transaction_seq: Some(transaction_seq),
    }
}

fn source(source_system: &str) -> SourceLocator {
    SourceLocator {
        source_system: source_system.to_string(),
        locator: format!("fixtures/{source_system}.jsonl"),
        fragment: Some("row-1".to_string()),
    }
}

fn sample_hash(seed: char) -> String {
    let hex = if seed.is_ascii_hexdigit() { seed } else { 'a' };
    format!("blake3:{}", hex.to_ascii_lowercase().to_string().repeat(64))
}
