#![forbid(unsafe_code)]

use canon::temporal::{
    AssertionStatus, CANON_IDENTITY_FACT_VERSION, FactScope, IdentityFact, IntervalBoundary,
    RecordedTime, SourceLocator, TemporalErrorCode, TimeInterval, canonical_fact_set_bytes,
    canonical_json_bytes, finalize_fact, finalize_facts,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.identity.fact.v1.schema.json");

#[test]
fn schema_declares_assertion_not_truth_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_IDENTITY_FACT_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_IDENTITY_FACT_VERSION
    );
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("not automatically trusted truth")
    );
    assert!(schema["properties"].get("conflict_key").is_some());
    assert!(schema["properties"].get("recorded_time").is_some());
}

#[test]
fn accepted_retracted_and_superseded_facts_round_trip() {
    let accepted = finalize_fact(sample_fact("person:alpha", AssertionStatus::Accepted))
        .expect("accepted fact finalizes");

    let retracted = finalize_fact(IdentityFact {
        recorded_time: RecordedTime {
            transaction_seq: Some(9),
            ..sample_recorded_time("2026-08-15T00:00:00Z")
        },
        materialization_digest: sample_hash('b'),
        assertion_status: AssertionStatus::Retracted,
        retracts: vec![accepted.fact_id.clone()],
        ..sample_fact("person:alpha", AssertionStatus::Retracted)
    })
    .expect("retraction fact finalizes");

    let superseding = finalize_fact(IdentityFact {
        object_id: "person:beta".to_string(),
        recorded_time: RecordedTime {
            transaction_seq: Some(10),
            ..sample_recorded_time("2026-08-16T00:00:00Z")
        },
        materialization_digest: sample_hash('c'),
        assertion_status: AssertionStatus::Accepted,
        supersedes: vec![accepted.fact_id.clone()],
        ..sample_fact("person:beta", AssertionStatus::Accepted)
    })
    .expect("superseding fact finalizes");

    let facts = finalize_facts(vec![
        accepted.clone(),
        retracted.clone(),
        superseding.clone(),
    ])
    .expect("fact set finalizes");
    assert_eq!(facts.len(), 3);
    assert_eq!(retracted.retracts, vec![accepted.fact_id.clone()]);
    assert_eq!(superseding.supersedes, vec![accepted.fact_id.clone()]);

    let json = serde_json::to_string(&superseding).expect("fact serializes");
    let round_tripped: IdentityFact = serde_json::from_str(&json).expect("fact deserializes");
    assert_eq!(round_tripped, superseding);
}

#[test]
fn late_arrival_distinguishes_valid_time_from_recorded_time() {
    let first = finalize_fact(sample_fact("person:alpha", AssertionStatus::Accepted))
        .expect("first fact finalizes");
    let later = finalize_fact(IdentityFact {
        recorded_time: sample_recorded_time("2026-09-01T09:30:00Z"),
        materialization_digest: sample_hash('d'),
        ..sample_fact("person:alpha", AssertionStatus::Accepted)
    })
    .expect("late-arriving fact finalizes");

    assert_eq!(first.assertion_key, later.assertion_key);
    assert_eq!(first.conflict_key, later.conflict_key);
    assert_ne!(first.fact_id, later.fact_id);
    assert_eq!(first.valid_time, later.valid_time);
    assert_ne!(first.recorded_time, later.recorded_time);
}

#[test]
fn equivalent_facts_dedup_but_conflicts_coexist() {
    let duplicate_left = sample_fact("person:alpha", AssertionStatus::Accepted);
    let duplicate_right = sample_fact("person:alpha", AssertionStatus::Accepted);
    let conflicting = sample_fact("person:beta", AssertionStatus::Accepted);

    let facts = finalize_facts(vec![duplicate_left, duplicate_right, conflicting])
        .expect("fact set finalizes");
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].conflict_key, facts[1].conflict_key);
    assert_ne!(facts[0].assertion_key, facts[1].assertion_key);
}

#[test]
fn canonical_serialization_is_clock_independent() {
    let local = IdentityFact {
        valid_time: TimeInterval {
            start_at: Some("2026-07-01T00:00:00-04:00".to_string()),
            end_at: Some("2026-07-31T23:59:59-04:00".to_string()),
            ..TimeInterval::default()
        },
        recorded_time: RecordedTime {
            start_at: Some("2026-08-01T00:15:00-04:00".to_string()),
            transaction_seq: Some(7),
            ..RecordedTime::default()
        },
        ..sample_fact("person:alpha", AssertionStatus::Accepted)
    };
    let utc = IdentityFact {
        valid_time: TimeInterval {
            start_at: Some("2026-07-01T04:00:00Z".to_string()),
            end_at: Some("2026-08-01T03:59:59Z".to_string()),
            ..TimeInterval::default()
        },
        recorded_time: RecordedTime {
            start_at: Some("2026-08-01T04:15:00Z".to_string()),
            transaction_seq: Some(7),
            ..RecordedTime::default()
        },
        ..sample_fact("person:alpha", AssertionStatus::Accepted)
    };

    let local_bytes = canonical_json_bytes(&local).expect("local fact serializes");
    let utc_bytes = canonical_json_bytes(&utc).expect("utc fact serializes");
    assert_eq!(local_bytes, utc_bytes);
}

#[test]
fn interval_boundary_and_recorded_time_invariants_hold() {
    let normalized = finalize_fact(IdentityFact {
        valid_time: TimeInterval {
            start_at: None,
            end_at: Some("2026-12-31T00:00:00Z".to_string()),
            start_bound: IntervalBoundary::Inclusive,
            end_bound: IntervalBoundary::Inclusive,
        },
        ..sample_fact("person:alpha", AssertionStatus::Accepted)
    })
    .expect("open start canonicalizes");
    assert_eq!(normalized.valid_time.start_bound, IntervalBoundary::Open);

    for invalid_valid_time in [
        TimeInterval {
            start_at: Some("2026-07-01T00:00:00Z".to_string()),
            end_at: Some("2026-06-30T23:59:59Z".to_string()),
            ..TimeInterval::default()
        },
        TimeInterval {
            start_at: Some("2026-07-01T00:00:00Z".to_string()),
            start_bound: IntervalBoundary::Exclusive,
            end_at: Some("2026-07-01T00:00:00Z".to_string()),
            end_bound: IntervalBoundary::Inclusive,
        },
    ] {
        let error = finalize_fact(IdentityFact {
            valid_time: invalid_valid_time,
            ..sample_fact("person:alpha", AssertionStatus::Accepted)
        })
        .expect_err("invalid interval must fail");
        assert_eq!(error.code, TemporalErrorCode::ArtifactContract);
    }

    let error = finalize_fact(IdentityFact {
        recorded_time: RecordedTime::default(),
        ..sample_fact("person:alpha", AssertionStatus::Accepted)
    })
    .expect_err("recorded_time without interval or sequence must fail");
    assert_eq!(error.code, TemporalErrorCode::ArtifactContract);
}

#[test]
fn supersession_and_retraction_link_invariants_hold() {
    let anchor = finalize_fact(sample_fact("person:alpha", AssertionStatus::Accepted))
        .expect("anchor fact finalizes");

    let error = finalize_fact(IdentityFact {
        assertion_status: AssertionStatus::Superseded,
        supersedes: Vec::new(),
        materialization_digest: sample_hash('e'),
        ..sample_fact("person:alpha", AssertionStatus::Superseded)
    })
    .expect_err("superseded fact without link must fail");
    assert_eq!(error.code, TemporalErrorCode::LinkInvariant);

    let error = finalize_fact(IdentityFact {
        assertion_status: AssertionStatus::Retracted,
        retracts: Vec::new(),
        materialization_digest: sample_hash('f'),
        ..sample_fact("person:alpha", AssertionStatus::Retracted)
    })
    .expect_err("retracted fact without link must fail");
    assert_eq!(error.code, TemporalErrorCode::LinkInvariant);

    let error = finalize_fact(IdentityFact {
        assertion_status: AssertionStatus::Accepted,
        supersedes: vec![anchor.fact_id.clone()],
        retracts: vec![anchor.fact_id],
        materialization_digest: sample_hash('0'),
        ..sample_fact("person:beta", AssertionStatus::Accepted)
    })
    .expect_err("overlapping supersedes and retracts must fail");
    assert_eq!(error.code, TemporalErrorCode::LinkInvariant);
}

#[test]
fn canonical_fact_set_bytes_are_stable() {
    let left = finalize_fact(sample_fact("person:alpha", AssertionStatus::Accepted))
        .expect("left fact finalizes");
    let right = finalize_fact(sample_fact("person:beta", AssertionStatus::Accepted))
        .expect("right fact finalizes");

    let bytes_a = canonical_fact_set_bytes(&[right.clone(), left.clone()])
        .expect("first ordering serializes");
    let bytes_b = canonical_fact_set_bytes(&[left, right]).expect("second ordering serializes");
    assert_eq!(bytes_a, bytes_b);
}

fn sample_fact(object_id: &str, assertion_status: AssertionStatus) -> IdentityFact {
    IdentityFact {
        version: String::new(),
        fact_id: String::new(),
        assertion_key: String::new(),
        conflict_key: String::new(),
        subject_id: "alias:issuer-name:sears".to_string(),
        predicate: "same_as".to_string(),
        object_id: object_id.to_string(),
        valid_time: TimeInterval {
            start_at: Some("2026-07-01T00:00:00Z".to_string()),
            start_bound: IntervalBoundary::Inclusive,
            end_at: Some("2026-07-31T23:59:59Z".to_string()),
            end_bound: IntervalBoundary::Inclusive,
        },
        recorded_time: sample_recorded_time("2026-08-01T09:30:00Z"),
        source_locator: SourceLocator {
            source_system: "fixture_catalog".to_string(),
            locator: "fixtures/temporal/issuer-facts.jsonl".to_string(),
            fragment: Some("row-1".to_string()),
        },
        materialization_digest: sample_hash('a'),
        assertion_status,
        trust_policy_ref: "trust.default.v1".to_string(),
        scope: Some(FactScope {
            scope_type: "jurisdiction".to_string(),
            scope_id: "us".to_string(),
        }),
        supersedes: Vec::new(),
        retracts: Vec::new(),
    }
}

fn sample_recorded_time(start_at: &str) -> RecordedTime {
    RecordedTime {
        start_at: Some(start_at.to_string()),
        start_bound: IntervalBoundary::Inclusive,
        end_at: None,
        end_bound: IntervalBoundary::Open,
        transaction_seq: Some(1),
    }
}

fn sample_hash(byte: char) -> String {
    format!("blake3:{}", byte.to_string().repeat(64))
}
