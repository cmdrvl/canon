#![forbid(unsafe_code)]

pub mod registry {
    pub use canon::registry::*;
}

pub use canon::RegistryDiffEntry;

mod temporal_impl {
    pub mod fact {
        #![allow(dead_code)]
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/temporal/fact.rs"));
    }

    pub use fact::*;

    pub fn finalize_fact(fact: fact::IdentityFact) -> fact::TemporalResult<fact::IdentityFact> {
        let canon_fact = serde_json::from_value(
            serde_json::to_value(fact).expect("local fact serializes to canon fact"),
        )
        .expect("local fact shape matches canon fact");
        let finalized = canon::temporal::finalize_fact(canon_fact).map_err(convert_error)?;
        serde_json::from_value(
            serde_json::to_value(finalized).expect("canon fact serializes to local fact"),
        )
        .map_err(|error| {
            fact::TemporalError::new(
                fact::TemporalErrorCode::ArtifactContract,
                format!("failed to convert finalized fact: {error}"),
            )
        })
    }

    pub fn finalize_facts(
        facts: impl IntoIterator<Item = fact::IdentityFact>,
    ) -> fact::TemporalResult<Vec<fact::IdentityFact>> {
        let canon_facts = facts
            .into_iter()
            .map(|fact| {
                serde_json::from_value(
                    serde_json::to_value(fact).expect("local fact serializes to canon fact"),
                )
                .expect("local fact shape matches canon fact")
            })
            .collect::<Vec<_>>();
        let finalized = canon::temporal::finalize_facts(canon_facts).map_err(convert_error)?;
        finalized
            .into_iter()
            .map(|fact| {
                serde_json::from_value(
                    serde_json::to_value(fact).expect("canon fact serializes to local fact"),
                )
                .map_err(|error| {
                    fact::TemporalError::new(
                        fact::TemporalErrorCode::ArtifactContract,
                        format!("failed to convert finalized fact: {error}"),
                    )
                })
            })
            .collect()
    }

    fn convert_error(error: canon::temporal::TemporalError) -> fact::TemporalError {
        serde_json::from_value(
            serde_json::to_value(error).expect("canon temporal error serializes"),
        )
        .expect("canon temporal error shape matches local temporal error")
    }

    pub mod alias {
        #![allow(dead_code)]
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/alias.rs"
        ));
    }

    pub mod conflict {
        #![allow(dead_code)]
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/conflict.rs"
        ));
    }

    pub mod compile {
        #![allow(dead_code)]
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/compile.rs"
        ));
    }

    pub mod explain {
        #![allow(dead_code)]
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/temporal/explain.rs"
        ));
    }

    pub mod diff {
        #![allow(dead_code)]
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/temporal/diff.rs"));
    }
}

use std::collections::BTreeMap;

use temporal_impl::alias::{
    AliasClaim, AliasScope, AliasValueKind, LookupVisibility, finalize_alias_claims,
};
use temporal_impl::compile::{
    CANON_TEMPORAL_COMPILE_VERSION, TemporalCompileRequest, canonical_compile_bytes,
    compile_exact_lookup_snapshot,
};
use temporal_impl::conflict::{CANON_TEMPORAL_CONFLICT_POLICY_VERSION, ConflictPolicy};
use temporal_impl::diff::{
    CANON_TEMPORAL_DIFF_VERSION, TemporalDiffFilter, TemporalDiffPageRequest, TemporalDiffRequest,
    diff_temporal_snapshots,
};
use temporal_impl::explain::{
    TemporalChangeClass, TemporalIdentitySnapshot, TemporalSnapshotReference,
};
use temporal_impl::fact::{
    AssertionStatus, FactScope, IdentityFact, IntervalBoundary, RecordedTime, SourceLocator,
    TimeInterval,
};
use temporal_impl::finalize_fact;

const SEEDS: [u64; 8] = [0x17, 0x2d, 0x41, 0x5b, 0x7f, 0x83, 0xa9, 0xd3];

#[test]
fn seeded_histories_match_reference_model_at_known_and_valid_boundaries() {
    for seed in SEEDS {
        let claims = generated_alias_claims(seed);
        for valid_at in [
            "2026-03-15T00:00:00Z",
            "2026-07-15T00:00:00Z",
            "2026-11-15T00:00:00Z",
        ] {
            for known_as_of in [
                "2026-02-15T00:00:00Z",
                "2026-06-15T00:00:00Z",
                "2026-10-15T00:00:00Z",
            ] {
                let artifact = compile_exact_lookup_snapshot(TemporalCompileRequest {
                    valid_at: valid_at.to_string(),
                    known_as_of: known_as_of.to_string(),
                    claims: claims.clone(),
                    ..compile_request(Vec::new())
                })
                .expect("generated snapshot compiles");
                let actual = compiled_mapping(&artifact);
                let expected = reference_mapping(&claims, valid_at, known_as_of);
                assert_eq!(
                    actual, expected,
                    "seed {seed:x} valid_at={valid_at} known_as_of={known_as_of}"
                );
            }
        }
    }
}

#[test]
fn permutation_duplicates_and_equivalent_shards_preserve_snapshot_bytes() {
    for seed in SEEDS {
        let claims = generated_alias_claims(seed);
        let base = snapshot_bytes(&claims);

        let mut reversed = claims.clone();
        reversed.reverse();
        assert_eq!(base, snapshot_bytes(&reversed), "seed {seed:x} reversed");

        let mut rotated = claims.clone();
        let rotation = (seed as usize) % rotated.len();
        rotated.rotate_left(rotation);
        assert_eq!(base, snapshot_bytes(&rotated), "seed {seed:x} rotated");

        let mut duplicated = claims.clone();
        duplicated.extend(claims.iter().take(4).cloned());
        assert_eq!(
            base,
            snapshot_bytes(&duplicated),
            "seed {seed:x} duplicated"
        );

        let (left, right): (Vec<_>, Vec<_>) = claims
            .iter()
            .cloned()
            .enumerate()
            .partition(|(index, _)| index % 2 == 0);
        let mut shard_merge = left.into_iter().map(|(_, claim)| claim).collect::<Vec<_>>();
        shard_merge.extend(right.into_iter().map(|(_, claim)| claim));
        assert_eq!(base, snapshot_bytes(&shard_merge), "seed {seed:x} shards");
    }
}

#[test]
fn known_time_corrections_and_valid_time_advances_change_only_target_surfaces() {
    let stable = identity_fact(
        FactInput::new("alias:stable", "org:stable", "feed_a")
            .valid_time(interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"))
            .recorded_at("2026-01-02T00:00:00Z", 1)
            .digest_seed('a'),
    );
    let original = finalize_fact(identity_fact(
        FactInput::new("alias:corrected", "org:alpha", "feed_a")
            .valid_time(interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"))
            .recorded_at("2026-01-02T00:00:00Z", 2)
            .digest_seed('b'),
    ))
    .expect("original finalizes");
    let correction = identity_fact(
        FactInput::new("alias:corrected", "org:beta", "feed_b")
            .valid_time(interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"))
            .recorded_at("2026-06-01T00:00:00Z", 3)
            .supersedes(vec![original.fact_id.clone()])
            .digest_seed('c'),
    );
    let expiring = identity_fact(
        FactInput::new("alias:seasonal", "org:seasonal", "feed_c")
            .valid_time(interval("2026-01-01T00:00:00Z", "2026-03-31T23:59:59Z"))
            .recorded_at("2026-01-02T00:00:00Z", 4)
            .digest_seed('d'),
    );
    let facts = vec![stable, original.clone(), correction, expiring];

    let before_known = identity_snapshot(
        "snapshot-before-known",
        "2026-02-01T00:00:00Z",
        "2026-02-01T00:00:00Z",
        facts.clone(),
    );
    let after_known = identity_snapshot(
        "snapshot-after-known",
        "2026-02-01T00:00:00Z",
        "2026-07-01T00:00:00Z",
        facts.clone(),
    );
    let correction_diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: before_known,
        after: after_known,
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest::default(),
        include_unchanged: false,
    })
    .expect("correction diff builds");
    assert_eq!(correction_diff.summary.changed_subject_count, 1);
    assert_eq!(correction_diff.changes[0].subject_id, "alias:corrected");
    assert_eq!(
        correction_diff.changes[0].change_class,
        TemporalChangeClass::Correction
    );

    let before_valid = identity_snapshot(
        "snapshot-before-valid",
        "2026-02-01T00:00:00Z",
        "2026-02-01T00:00:00Z",
        facts.clone(),
    );
    let after_valid = identity_snapshot(
        "snapshot-after-valid",
        "2026-05-01T00:00:00Z",
        "2026-02-01T00:00:00Z",
        facts,
    );
    let expiry_diff = diff_temporal_snapshots(TemporalDiffRequest {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before: before_valid,
        after: after_valid,
        filter: TemporalDiffFilter::default(),
        page: TemporalDiffPageRequest::default(),
        include_unchanged: false,
    })
    .expect("expiry diff builds");
    assert_eq!(expiry_diff.summary.changed_subject_count, 1);
    assert_eq!(expiry_diff.changes[0].subject_id, "alias:seasonal");
    assert_eq!(
        expiry_diff.changes[0].change_class,
        TemporalChangeClass::ExpiredFact
    );
}

fn generated_alias_claims(seed: u64) -> Vec<AliasClaim> {
    let mut rng = Lcg::new(seed);
    (0..12)
        .map(|index| {
            let start_month = 1 + (rng.next_u64() % 6) as u8;
            let duration = 4 + (rng.next_u64() % 4) as u8;
            let end_month = (start_month + duration).min(12);
            let recorded_month = 1 + (rng.next_u64() % 9) as u8;
            let alias_value = format!("SURFACE-{seed:02x}-{index:02}");
            let entity_id = format!("entity:{seed:02x}-{index:02}");
            AliasClaim {
                version: "canon.temporal.alias.v1".to_string(),
                claim_id: String::new(),
                claim_key: String::new(),
                conflict_key: String::new(),
                alias_value,
                alias_kind: AliasValueKind::Name,
                entity_id,
                lookup_visibility: LookupVisibility::Global,
                scope: AliasScope::default(),
                valid_time: interval(
                    &timestamp(start_month, 1),
                    &format!("2026-{end_month:02}-28T23:59:59Z"),
                ),
                recorded_time: recorded_time(&timestamp(recorded_month, 2), index as u64 + 1),
                source_locator: SourceLocator {
                    source_system: format!("feed_{}", index % 3),
                    locator: format!("fixtures/generated/{seed:02x}/{index:02}.jsonl"),
                    fragment: Some(format!("row-{index}")),
                },
                materialization_digest: digest_from(seed, index as u64),
                assertion_status: AssertionStatus::Accepted,
                trust_policy_ref: "trust.generated.v1".to_string(),
                promoted_to_global_by: None,
                trusted_anchor: None,
                supersedes: Vec::new(),
                retracts: Vec::new(),
            }
        })
        .collect()
}

fn reference_mapping(
    claims: &[AliasClaim],
    valid_at: &str,
    known_as_of: &str,
) -> BTreeMap<String, String> {
    finalize_alias_claims(claims.to_vec())
        .expect("claims finalize")
        .into_iter()
        .filter(|claim| claim.lookup_visibility == LookupVisibility::Global)
        .filter(|claim| contains(&claim.valid_time, valid_at))
        .filter(|claim| recorded_contains(&claim.recorded_time, known_as_of))
        .map(|claim| (claim.alias_value, claim.entity_id))
        .collect()
}

fn snapshot_bytes(claims: &[AliasClaim]) -> Vec<u8> {
    let artifact = compile_exact_lookup_snapshot(TemporalCompileRequest {
        valid_at: "2026-07-15T00:00:00Z".to_string(),
        known_as_of: "2026-10-15T00:00:00Z".to_string(),
        claims: claims.to_vec(),
        ..compile_request(Vec::new())
    })
    .expect("snapshot compiles");
    canonical_compile_bytes(&artifact).expect("canonical compile bytes")
}

fn compiled_mapping(
    artifact: &temporal_impl::compile::TemporalCompileArtifact,
) -> BTreeMap<String, String> {
    artifact
        .registry_package
        .lookup_entries
        .iter()
        .map(|entry| (entry.input.clone(), entry.canonical_id.clone()))
        .collect()
}

fn compile_request(claims: Vec<AliasClaim>) -> TemporalCompileRequest {
    TemporalCompileRequest {
        version: CANON_TEMPORAL_COMPILE_VERSION.to_string(),
        registry_id: "temporal-properties".to_string(),
        registry_version: "1.0.0".to_string(),
        valid_at: "2026-07-15T00:00:00Z".to_string(),
        known_as_of: "2026-10-15T00:00:00Z".to_string(),
        policy: ConflictPolicy {
            version: CANON_TEMPORAL_CONFLICT_POLICY_VERSION.to_string(),
            policy_id: "abstain-by-default".to_string(),
            clauses: Vec::new(),
        },
        claims,
        scope_filter: None,
        parent_snapshot: None,
        relation_sidecars: Vec::new(),
        canonical_iri_namespace: Some("https://canon.example.test/entity/".to_string()),
    }
}

fn identity_snapshot(
    snapshot_id: &str,
    valid_at: &str,
    known_as_of: &str,
    facts: Vec<IdentityFact>,
) -> TemporalIdentitySnapshot {
    TemporalIdentitySnapshot {
        snapshot: TemporalSnapshotReference {
            snapshot_id: snapshot_id.to_string(),
            registry_id: "temporal-properties".to_string(),
            registry_version: "1.0.0".to_string(),
            compiled_snapshot_digest: digest_from(snapshot_id.len() as u64, 99),
            valid_at: valid_at.to_string(),
            known_as_of: known_as_of.to_string(),
            policy_ref: "policy.properties".to_string(),
            policy_version: "1".to_string(),
        },
        facts,
        relationships: Vec::new(),
    }
}

struct FactInput<'a> {
    subject_id: &'a str,
    object_id: &'a str,
    source_system: &'a str,
    valid_time: TimeInterval,
    recorded_at: &'a str,
    transaction_seq: u64,
    assertion_status: AssertionStatus,
    supersedes: Vec<String>,
    retracts: Vec<String>,
    digest_seed: char,
}

impl<'a> FactInput<'a> {
    fn new(subject_id: &'a str, object_id: &'a str, source_system: &'a str) -> Self {
        Self {
            subject_id,
            object_id,
            source_system,
            valid_time: interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"),
            recorded_at: "2026-01-02T00:00:00Z",
            transaction_seq: 1,
            assertion_status: AssertionStatus::Accepted,
            supersedes: Vec::new(),
            retracts: Vec::new(),
            digest_seed: 'a',
        }
    }

    fn valid_time(mut self, valid_time: TimeInterval) -> Self {
        self.valid_time = valid_time;
        self
    }

    fn recorded_at(mut self, recorded_at: &'a str, transaction_seq: u64) -> Self {
        self.recorded_at = recorded_at;
        self.transaction_seq = transaction_seq;
        self
    }

    fn supersedes(mut self, supersedes: Vec<String>) -> Self {
        self.supersedes = supersedes;
        self
    }

    fn digest_seed(mut self, digest_seed: char) -> Self {
        self.digest_seed = digest_seed;
        self
    }
}

fn identity_fact(input: FactInput<'_>) -> IdentityFact {
    IdentityFact {
        version: String::new(),
        fact_id: String::new(),
        assertion_key: String::new(),
        conflict_key: String::new(),
        subject_id: input.subject_id.to_string(),
        predicate: "same_as".to_string(),
        object_id: input.object_id.to_string(),
        valid_time: input.valid_time,
        recorded_time: recorded_time(input.recorded_at, input.transaction_seq),
        source_locator: SourceLocator {
            source_system: input.source_system.to_string(),
            locator: format!("fixtures/{}.jsonl", input.source_system),
            fragment: Some(format!("row-{}", input.transaction_seq)),
        },
        materialization_digest: sample_hash(input.digest_seed),
        assertion_status: input.assertion_status,
        trust_policy_ref: "trust.properties.v1".to_string(),
        scope: Some(FactScope {
            scope_type: "portfolio".to_string(),
            scope_id: "neutral".to_string(),
        }),
        supersedes: input.supersedes,
        retracts: input.retracts,
    }
}

fn contains(interval: &TimeInterval, at: &str) -> bool {
    if let Some(start_at) = interval.start_at.as_deref() {
        if at < start_at {
            return false;
        }
        if at == start_at && matches!(interval.start_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    if let Some(end_at) = interval.end_at.as_deref() {
        if at > end_at {
            return false;
        }
        if at == end_at && matches!(interval.end_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    true
}

fn recorded_contains(recorded_time: &RecordedTime, known_as_of: &str) -> bool {
    if let Some(start_at) = recorded_time.start_at.as_deref() {
        if known_as_of < start_at {
            return false;
        }
        if known_as_of == start_at
            && matches!(recorded_time.start_bound, IntervalBoundary::Exclusive)
        {
            return false;
        }
    }
    if let Some(end_at) = recorded_time.end_at.as_deref() {
        if known_as_of > end_at {
            return false;
        }
        if known_as_of == end_at && matches!(recorded_time.end_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    true
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

fn timestamp(month: u8, day: u8) -> String {
    format!("2026-{month:02}-{day:02}T00:00:00Z")
}

fn sample_hash(seed: char) -> String {
    let hex = if seed.is_ascii_hexdigit() { seed } else { 'a' };
    format!("blake3:{}", hex.to_ascii_lowercase().to_string().repeat(64))
}

fn digest_from(seed: u64, index: u64) -> String {
    let mut bytes = Vec::new();
    bytes.extend(seed.to_le_bytes());
    bytes.extend(index.to_le_bytes());
    format!("blake3:{}", blake3::hash(&bytes).to_hex())
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }
}
