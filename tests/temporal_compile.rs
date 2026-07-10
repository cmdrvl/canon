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
}

use registry::validate_registry_package;
use serde_json::Value;
use temporal_impl::alias::{
    AliasClaim, AliasScope, AliasValueKind, LookupVisibility, PromotionProvenance,
    finalize_alias_claim,
};
use temporal_impl::compile::{
    CANON_TEMPORAL_COMPILE_VERSION, CompileScopeFilter, TemporalCompileOmissionReason,
    TemporalCompileRequest, TemporalRelationSidecar, canonical_compile_bytes,
    compile_exact_lookup_snapshot,
};
use temporal_impl::conflict::{CANON_TEMPORAL_CONFLICT_POLICY_VERSION, ConflictPolicy};
use temporal_impl::fact::{
    AssertionStatus, IntervalBoundary, RecordedTime, SourceLocator, TimeInterval,
};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.temporal.compile.v1.schema.json");

#[test]
fn schema_declares_compile_contract_and_registry_package_surface() {
    let schema = serde_json::from_str::<Value>(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_TEMPORAL_COMPILE_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_TEMPORAL_COMPILE_VERSION
    );
    assert_eq!(
        schema["properties"]["registry_package"]["$ref"],
        "#/$defs/registry_package"
    );
    assert_eq!(
        schema["$defs"]["registry_package"]["properties"]["schema_version"]["const"],
        "canon.registry.package.v1"
    );
    assert_eq!(
        schema["x-canon-contract"]["relation_sidecars_affect_lookup"],
        false
    );
    assert!(
        schema["x-canon-contract"]["backend_compatibility"]
            .as_array()
            .expect("backend compatibility array")
            .iter()
            .any(|value| value == "search-index")
    );
}

#[test]
fn compile_is_deterministic_across_input_order_and_validates_package_firewall() {
    let request_a = compile_request(vec![
        global_claim("BETA", "ent:beta", "feed_b"),
        global_claim("ALPHA", "ent:alpha", "feed_a"),
    ]);
    let request_b = compile_request(vec![
        global_claim("ALPHA", "ent:alpha", "feed_a"),
        global_claim("BETA", "ent:beta", "feed_b"),
    ]);

    let artifact_a = compile_exact_lookup_snapshot(request_a).expect("artifact a compiles");
    let artifact_b = compile_exact_lookup_snapshot(request_b).expect("artifact b compiles");

    validate_registry_package(&artifact_a.registry_package).expect("package validates");
    assert_eq!(
        canonical_compile_bytes(&artifact_a).expect("artifact a bytes"),
        canonical_compile_bytes(&artifact_b).expect("artifact b bytes")
    );
    assert_eq!(artifact_a.registry_package.lookup_entries.len(), 2);
    assert_eq!(artifact_a.mapping_proofs.len(), 2);
    assert_eq!(artifact_a.registry_package.lookup_entries[0].input, "ALPHA");
    assert_eq!(artifact_a.registry_package.lookup_entries[1].input, "BETA");
    eprintln!(
        "deterministic snapshot: {} lookup entries, {} omissions",
        artifact_a.registry_package.lookup_entries.len(),
        artifact_a.omissions.len()
    );
}

#[test]
fn known_as_of_correction_changes_compiled_mapping_and_proof() {
    let original = finalize_alias_claim(global_claim("ACME HOLDINGS", "ent:alpha", "source_a"))
        .expect("original claim finalizes");
    let correction = finalize_alias_claim(AliasClaim {
        entity_id: "ent:beta".to_string(),
        recorded_time: recorded_time("2026-02-01T00:00:00Z", 2),
        supersedes: vec![original.claim_id.clone()],
        ..global_claim("ACME HOLDINGS", "ent:beta", "source_a")
    })
    .expect("correction claim finalizes");

    let before = compile_exact_lookup_snapshot(TemporalCompileRequest {
        known_as_of: "2026-01-15T00:00:00Z".to_string(),
        claims: vec![original.clone(), correction.clone()],
        ..compile_request(Vec::new())
    })
    .expect("pre-correction snapshot compiles");
    let after = compile_exact_lookup_snapshot(TemporalCompileRequest {
        known_as_of: "2026-02-15T00:00:00Z".to_string(),
        claims: vec![original.clone(), correction.clone()],
        ..compile_request(Vec::new())
    })
    .expect("post-correction snapshot compiles");

    assert_eq!(before.registry_package.lookup_entries.len(), 1);
    assert_eq!(
        before.registry_package.lookup_entries[0].canonical_id,
        "ent:alpha"
    );
    assert_eq!(
        after.registry_package.lookup_entries[0].canonical_id,
        "ent:beta"
    );
    assert_eq!(after.mapping_proofs[0].claim_id, correction.claim_id);
    assert_eq!(
        after.mapping_proofs[0].recorded_time.transaction_seq,
        Some(2)
    );
}

#[test]
fn abstaining_conflicts_emit_structured_omissions_and_no_last_write_wins() {
    let left = global_claim("ACME", "ent:alpha", "source_a");
    let right = AliasClaim {
        recorded_time: recorded_time("2026-01-03T00:00:00Z", 2),
        ..global_claim("ACME", "ent:beta", "source_b")
    };

    let artifact = compile_exact_lookup_snapshot(compile_request(vec![left, right]))
        .expect("abstaining artifact compiles");

    assert!(artifact.registry_package.lookup_entries.is_empty());
    let omission = artifact
        .omissions
        .iter()
        .find(|omission| {
            omission.reason == TemporalCompileOmissionReason::ConflictAbstained
                && omission.claim_ids.len() == 2
        })
        .expect("conflict omission");
    assert!(omission.message.contains("abstain"));
    eprintln!(
        "abstained snapshot: {} lookup entries, {} omissions",
        artifact.registry_package.lookup_entries.len(),
        artifact.omissions.len()
    );
}

#[test]
fn scope_filter_parent_snapshot_and_relation_sidecars_stay_projection_only() {
    let included = finalize_alias_claim(promoted_global_claim(
        "LOCAL-ALPHA",
        "ent:alpha",
        "feed_a",
        "product_line",
        "book_a",
        "promote-feed-a",
    ))
    .expect("included claim finalizes");
    let excluded = finalize_alias_claim(promoted_global_claim(
        "LOCAL-BETA",
        "ent:beta",
        "feed_b",
        "product_line",
        "book_b",
        "promote-feed-b",
    ))
    .expect("excluded claim finalizes");

    let artifact = compile_exact_lookup_snapshot(TemporalCompileRequest {
        scope_filter: Some(CompileScopeFilter {
            source_systems: vec!["feed_a".to_string()],
            scope_type: Some("product_line".to_string()),
            scope_id: Some("book_a".to_string()),
        }),
        parent_snapshot: Some(canon::registry::RegistryPackageDependencyReference {
            id: "canon-registry-parent".to_string(),
            version: "0.9.0".to_string(),
            content_digest:
                "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    .to_string(),
        }),
        relation_sidecars: vec![TemporalRelationSidecar {
            schema_version: "canon.identity.relation.v1".to_string(),
            path: "sidecars/relations.json".to_string(),
            content_digest:
                "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                    .to_string(),
            relation_count: 2,
        }],
        claims: vec![included.clone(), excluded.clone()],
        ..compile_request(Vec::new())
    })
    .expect("scoped artifact compiles");

    assert_eq!(artifact.registry_package.lookup_entries.len(), 1);
    assert_eq!(
        artifact.registry_package.lookup_entries[0].canonical_id,
        "ent:alpha"
    );
    assert_eq!(
        artifact.mapping_proofs[0].scope.scope_id.as_deref(),
        Some("book_a")
    );
    assert_eq!(
        artifact.mapping_proofs[0].policy_clause_ids,
        vec!["promote-feed-a".to_string()]
    );
    assert_eq!(artifact.registry_package.dependency_references.len(), 1);
    assert_eq!(
        artifact
            .parent_snapshot
            .as_ref()
            .expect("parent snapshot")
            .id,
        "canon-registry-parent"
    );
    assert!(artifact.omissions.iter().any(|omission| {
        omission.reason == TemporalCompileOmissionReason::ScopeExcluded
            && omission.claim_ids == vec![excluded.claim_id.clone()]
    }));
    assert!(artifact.omissions.iter().any(|omission| {
        omission.reason == TemporalCompileOmissionReason::RelationSidecarOnly
            && omission.subject_key == "sidecars/relations.json"
    }));
}

fn compile_request(claims: Vec<AliasClaim>) -> TemporalCompileRequest {
    TemporalCompileRequest {
        version: CANON_TEMPORAL_COMPILE_VERSION.to_string(),
        registry_id: "temporal-snapshot".to_string(),
        registry_version: "1.0.0".to_string(),
        valid_at: "2026-03-01T00:00:00Z".to_string(),
        known_as_of: "2026-03-01T00:00:00Z".to_string(),
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

fn global_claim(alias_value: &str, entity_id: &str, source_system: &str) -> AliasClaim {
    AliasClaim {
        version: "canon.temporal.alias.v1".to_string(),
        claim_id: String::new(),
        claim_key: String::new(),
        conflict_key: String::new(),
        alias_value: alias_value.to_string(),
        alias_kind: AliasValueKind::Name,
        entity_id: entity_id.to_string(),
        lookup_visibility: LookupVisibility::Global,
        scope: AliasScope::default(),
        valid_time: interval("2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z"),
        recorded_time: recorded_time("2026-01-02T00:00:00Z", 1),
        source_locator: SourceLocator {
            source_system: source_system.to_string(),
            locator: format!("fixtures/{source_system}.jsonl"),
            fragment: Some("row-1".to_string()),
        },
        materialization_digest: sample_hash('a'),
        assertion_status: AssertionStatus::Accepted,
        trust_policy_ref: "trust.default.v1".to_string(),
        promoted_to_global_by: None,
        trusted_anchor: None,
        supersedes: Vec::new(),
        retracts: Vec::new(),
    }
}

fn promoted_global_claim(
    alias_value: &str,
    entity_id: &str,
    source_system: &str,
    scope_type: &str,
    scope_id: &str,
    promotion_clause_id: &str,
) -> AliasClaim {
    AliasClaim {
        scope: AliasScope {
            source_system: Some(source_system.to_string()),
            scope_type: Some(scope_type.to_string()),
            scope_id: Some(scope_id.to_string()),
        },
        promoted_to_global_by: Some(PromotionProvenance {
            policy_clause_id: promotion_clause_id.to_string(),
            evidence_ref: "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_string(),
        }),
        ..global_claim(alias_value, entity_id, source_system)
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

fn sample_hash(hex_digit: char) -> String {
    format!("blake3:{}", hex_digit.to_string().repeat(64))
}
