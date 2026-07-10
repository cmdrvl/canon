#![forbid(unsafe_code)]

#[path = "../src/evidence/mod.rs"]
mod evidence;

use evidence::{
    CANON_EVIDENCE_VERSION, EvidenceAuthorityBasis, EvidenceBundle, EvidenceCategoricalMeasurement,
    EvidenceExtension, EvidenceExtensionValue, EvidenceKind, EvidenceMeasurement,
    EvidenceNumericMeasurement, EvidenceOperatorRef, EvidencePolicyRef, EvidenceProvenanceRef,
    EvidenceRecord, EvidenceScope, EvidenceTarget, canonical_bundle_bytes, canonicalize_bundle,
    canonicalize_record, merge_shards,
};
use serde_json::Value;
use std::collections::BTreeMap;

const EVIDENCE_SCHEMA_JSON: &str = include_str!("../schemas/canon.evidence.v1.schema.json");

#[test]
fn evidence_contract_round_trips_every_major_variant() {
    let bundle = canonicalize_bundle(sample_records()).expect("bundle canonicalizes");
    assert_eq!(bundle.version, CANON_EVIDENCE_VERSION);
    assert_eq!(bundle.record_count, 9);
    assert_eq!(bundle.records.len(), 9);
    assert!(
        bundle
            .records
            .windows(2)
            .all(|window| window[0].evidence_id < window[1].evidence_id)
    );

    let by_kind = bundle
        .records
        .iter()
        .map(|record| record.kind.clone())
        .collect::<Vec<_>>();
    assert!(by_kind.contains(&EvidenceKind::Observation));
    assert!(by_kind.contains(&EvidenceKind::CandidateScope));
    assert!(by_kind.contains(&EvidenceKind::PairSupport));
    assert!(by_kind.contains(&EvidenceKind::HyperedgeSupport));
    assert!(by_kind.contains(&EvidenceKind::RecordLinkSupport));
    assert!(by_kind.contains(&EvidenceKind::ContextOnly));
    assert!(by_kind.contains(&EvidenceKind::ContextualNegative));
    assert!(by_kind.contains(&EvidenceKind::Missingness));
    assert!(by_kind.contains(&EvidenceKind::AntiMergeVeto));

    let value = serde_json::to_value(&bundle).expect("bundle serializes");
    let round_tripped: EvidenceBundle = serde_json::from_value(value).expect("bundle deserializes");
    assert_eq!(round_tripped, bundle);
}

#[test]
fn evidence_contract_merge_is_stable_across_shards_and_input_order() {
    let records = sample_records();
    let canonical = canonicalize_bundle(records.clone()).expect("bundle canonicalizes");

    let shards = vec![
        vec![records[4].clone(), records[1].clone(), records[8].clone()],
        vec![records[0].clone(), records[6].clone()],
        vec![
            records[3].clone(),
            records[2].clone(),
            records[5].clone(),
            records[7].clone(),
        ],
        vec![records[0].clone()],
    ];
    let merged = merge_shards(shards).expect("shards merge canonically");

    assert_eq!(merged, canonical);
    assert_eq!(
        canonical_bundle_bytes(&merged).expect("bundle bytes"),
        canonical_bundle_bytes(&canonical).expect("bundle bytes"),
    );
}

#[test]
fn evidence_contract_rejects_missing_authority_reserved_extensions_and_provenance_gaps() {
    let mut veto = sample_anti_merge_veto();
    veto.authority_basis = None;
    let error = canonicalize_record(veto).expect_err("veto without authority must refuse");
    assert_eq!(error.code, evidence::EvidenceErrorCode::ArtifactContract);

    let mut reserved_extension = sample_context_only();
    reserved_extension.extensions = vec![EvidenceExtension {
        namespace: "adapter.example".to_string(),
        schema_ref: "https://example.com/schema".to_string(),
        payload: BTreeMap::from([(
            "authority".to_string(),
            EvidenceExtensionValue::String("merge".to_string()),
        )]),
    }];
    let error =
        canonicalize_record(reserved_extension).expect_err("reserved extension key must refuse");
    assert_eq!(error.code, evidence::EvidenceErrorCode::InvalidExtension);

    let mut missing_provenance = sample_pair_support();
    missing_provenance.provenance.clear();
    let error = canonicalize_record(missing_provenance).expect_err("provenance gap must refuse");
    assert_eq!(error.code, evidence::EvidenceErrorCode::ArtifactContract);
}

#[test]
fn evidence_contract_schema_declares_extension_firewall_and_variant_surface() {
    let schema = serde_json::from_str::<Value>(EVIDENCE_SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_EVIDENCE_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_EVIDENCE_VERSION
    );
    assert_eq!(schema["additionalProperties"], false);

    let kind_enum = schema["$defs"]["evidence_kind"]["enum"]
        .as_array()
        .expect("kind enum");
    for expected in [
        "observation",
        "candidate_scope",
        "pair_support",
        "hyperedge_support",
        "record_link_support",
        "context_only",
        "contextual_negative",
        "missingness",
        "anti_merge_veto",
    ] {
        assert!(
            kind_enum.iter().any(|value| value == expected),
            "schema missing evidence kind {expected}"
        );
    }

    let target_variants = schema["$defs"]["evidence_target"]["oneOf"]
        .as_array()
        .expect("target variants");
    assert!(target_variants.len() >= 5);
    assert!(
        schema["$defs"]["evidence_extension"]["properties"]
            .as_object()
            .expect("extension properties")
            .contains_key("schema_ref")
    );
}

#[test]
fn evidence_contract_golden_bytes_and_hash_are_stable() {
    let bundle = canonicalize_bundle(vec![sample_pair_support(), sample_anti_merge_veto()])
        .expect("golden bundle canonicalizes");
    let bytes = canonical_bundle_bytes(&bundle).expect("bundle bytes");
    let actual = String::from_utf8(bytes).expect("utf8 json");

    let expected = r#"{"version":"canon.evidence.v1","record_count":2,"content_hash":"blake3:6d7d3a2d6349a2c90849dce6b783d3732dc83349dd826376da7b0b0b3a92708e","records":[{"version":"canon.evidence.v1","evidence_id":"evidence:blake3:0445b33d4b1aafdee239c691517f1d83ef92a99742b08c25a62825514e069901","kind":"pair_support","target":{"target_kind":"pair","left_id":"surface:acme","right_id":"surface:acme_holdings"},"operator":{"namespace":"namekit","operator_id":"exact_view:core_name","operator_version":"1.0.0","adapter_id":"native"},"reason_code":"exact_name_match","policy":{"policy_id":"tenant_label.default","policy_version":"0.1.0","content_hash":"blake3:policy"},"scope":{"scope_type":"profile","scope_id":"cmbs_tenant_label","namespace":"entity"},"temporal_scope":{"as_of":"2026-06-30"},"provenance":[{"source_type":"filing","source_id":"10d:acme","locator":"rows/17","content_hash":"blake3:source-a","observed_at":"2026-06-30"}],"measurements":[{"kind":"categorical","feature_id":"view_name","value":"tenant_core"},{"kind":"numeric","feature_id":"score_units","units":"score_units","scaled_value":10000,"scale":0}],"extensions":[{"namespace":"adapter.example","schema_ref":"https://example.com/evidence-extension.schema.json","payload":{"explanation":"exact core-name agreement","inputs":["acme","acme holdings"]}}]},{"version":"canon.evidence.v1","evidence_id":"evidence:blake3:8515fb77ce330641677f1e7a4d0c176ee61bee59e1375c9d7f7c492b81c99d57","kind":"anti_merge_veto","target":{"target_kind":"pair","left_id":"surface:idera_inc","right_id":"surface:idera_pharma"},"operator":{"namespace":"review","operator_id":"cannot_link:reviewed_distinctness","operator_version":"1.0.0","adapter_id":"native"},"reason_code":"reviewed_distinctness","policy":{"policy_id":"tenant_label.default","policy_version":"0.1.0","content_hash":"blake3:policy"},"authority_basis":"reviewed_constraint","scope":{"scope_type":"profile","scope_id":"cmbs_tenant_label","namespace":"entity"},"temporal_scope":{"as_of":"2026-06-30"},"provenance":[{"source_type":"review_queue","source_id":"rq:idera","locator":"decision/4","content_hash":"blake3:source-veto","observed_at":"2026-07-01"}],"measurements":[{"kind":"categorical","feature_id":"protected_tokens","value":"inc_vs_pharmaceuticals"},{"kind":"numeric","feature_id":"score_units","units":"score_units","scaled_value":10000,"scale":0}]}]}"#;

    assert_eq!(actual, expected);
}

fn sample_records() -> Vec<EvidenceRecord> {
    vec![
        sample_observation(),
        sample_candidate_scope(),
        sample_pair_support(),
        sample_hyperedge_support(),
        sample_record_link_support(),
        sample_context_only(),
        sample_contextual_negative(),
        sample_missingness(),
        sample_anti_merge_veto(),
    ]
}

fn sample_observation() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::Observation,
        target: EvidenceTarget::Observation {
            observation_id: "obs:rocket_bidco".to_string(),
            surface: "Rocket Bidco, Inc. (dba Recochem)".to_string(),
            subject_hint: Some("issuer".to_string()),
        },
        operator: native_operator("profile_projection", "1.0.0"),
        reason_code: "observed_surface".to_string(),
        policy: policy_ref(),
        authority_basis: None,
        scope: Some(profile_scope()),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "filing",
            "10d:recochem",
            "rows/3",
            "blake3:source-obs",
        )],
        measurements: vec![EvidenceMeasurement::Categorical(
            EvidenceCategoricalMeasurement {
                feature_id: "surface_role".to_string(),
                value: "legal_with_dba".to_string(),
            },
        )],
        extensions: vec![],
    }
}

fn sample_candidate_scope() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::CandidateScope,
        target: EvidenceTarget::CandidateScope {
            scope_id: "scope:recochem".to_string(),
            candidate_ids: vec![
                "cand:recochem".to_string(),
                "cand:rocket_bidco".to_string(),
                "cand:recochem".to_string(),
            ],
        },
        operator: native_operator("candidate_scope", "1.0.0"),
        reason_code: "candidate_recall".to_string(),
        policy: policy_ref(),
        authority_basis: None,
        scope: Some(profile_scope()),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "candidate_index",
            "index:recochem",
            "bucket/core_name",
            "blake3:source-scope",
        )],
        measurements: vec![EvidenceMeasurement::Numeric(EvidenceNumericMeasurement {
            feature_id: "candidate_count".to_string(),
            units: "count".to_string(),
            scaled_value: 2,
            scale: 0,
        })],
        extensions: vec![],
    }
}

fn sample_pair_support() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::PairSupport,
        target: EvidenceTarget::Pair {
            left_id: "surface:acme_holdings".to_string(),
            right_id: "surface:acme".to_string(),
        },
        operator: native_operator("exact_view:core_name", "1.0.0"),
        reason_code: "exact_name_match".to_string(),
        policy: policy_ref(),
        authority_basis: None,
        scope: Some(profile_scope()),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "filing",
            "10d:acme",
            "rows/17",
            "blake3:source-a",
        )],
        measurements: vec![
            EvidenceMeasurement::Numeric(EvidenceNumericMeasurement {
                feature_id: "score_units".to_string(),
                units: "score_units".to_string(),
                scaled_value: 10_000,
                scale: 0,
            }),
            EvidenceMeasurement::Categorical(EvidenceCategoricalMeasurement {
                feature_id: "view_name".to_string(),
                value: "tenant_core".to_string(),
            }),
        ],
        extensions: vec![EvidenceExtension {
            namespace: "adapter.example".to_string(),
            schema_ref: "https://example.com/evidence-extension.schema.json".to_string(),
            payload: BTreeMap::from([
                (
                    "explanation".to_string(),
                    EvidenceExtensionValue::String("exact core-name agreement".to_string()),
                ),
                (
                    "inputs".to_string(),
                    EvidenceExtensionValue::List(vec![
                        EvidenceExtensionValue::String("acme".to_string()),
                        EvidenceExtensionValue::String("acme holdings".to_string()),
                    ]),
                ),
            ]),
        }],
    }
}

fn sample_hyperedge_support() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::HyperedgeSupport,
        target: EvidenceTarget::Hyperedge {
            member_ids: vec![
                "surface:recochem_holdings".to_string(),
                "surface:rocket_bidco".to_string(),
                "surface:recochem".to_string(),
            ],
        },
        operator: native_operator("dba_surface_cluster", "1.0.0"),
        reason_code: "shared_dba_surface".to_string(),
        policy: policy_ref(),
        authority_basis: None,
        scope: Some(profile_scope()),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "filing",
            "10d:recochem",
            "rows/3",
            "blake3:source-h",
        )],
        measurements: vec![EvidenceMeasurement::Numeric(EvidenceNumericMeasurement {
            feature_id: "member_count".to_string(),
            units: "count".to_string(),
            scaled_value: 3,
            scale: 0,
        })],
        extensions: vec![],
    }
}

fn sample_record_link_support() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::RecordLinkSupport,
        target: EvidenceTarget::RecordLink {
            left_source: "bdc_a".to_string(),
            left_record_id: "holding:17".to_string(),
            right_source: "bdc_b".to_string(),
            right_record_id: "holding:41".to_string(),
        },
        operator: native_operator("cross_holder_linkage", "1.0.0"),
        reason_code: "same_quarter_shared_holder_graph".to_string(),
        policy: policy_ref(),
        authority_basis: None,
        scope: Some(EvidenceScope {
            scope_type: "quarter".to_string(),
            scope_id: "2026q2".to_string(),
            namespace: Some("bdc".to_string()),
        }),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "assignment_graph",
            "cohold:recochem",
            "edge/3",
            "blake3:source-link",
        )],
        measurements: vec![EvidenceMeasurement::Numeric(EvidenceNumericMeasurement {
            feature_id: "distinct_holders".to_string(),
            units: "count".to_string(),
            scaled_value: 3,
            scale: 0,
        })],
        extensions: vec![],
    }
}

fn sample_context_only() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::ContextOnly,
        target: EvidenceTarget::Pair {
            left_id: "surface:recochem".to_string(),
            right_id: "surface:rocket_bidco".to_string(),
        },
        operator: native_operator("coholder_context", "1.0.0"),
        reason_code: "three_distinct_filers_same_quarter".to_string(),
        policy: policy_ref(),
        authority_basis: None,
        scope: Some(EvidenceScope {
            scope_type: "quarter".to_string(),
            scope_id: "2026q2".to_string(),
            namespace: Some("bdc".to_string()),
        }),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "assignment_graph",
            "cohold:recochem",
            "summary/2",
            "blake3:source-context",
        )],
        measurements: vec![EvidenceMeasurement::Numeric(EvidenceNumericMeasurement {
            feature_id: "distinct_holders".to_string(),
            units: "count".to_string(),
            scaled_value: 3,
            scale: 0,
        })],
        extensions: vec![],
    }
}

fn sample_contextual_negative() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::ContextualNegative,
        target: EvidenceTarget::Pair {
            left_id: "surface:idera_inc".to_string(),
            right_id: "surface:idera_pharma".to_string(),
        },
        operator: native_operator("industry_mismatch", "1.0.0"),
        reason_code: "industry_context_disagrees".to_string(),
        policy: policy_ref(),
        authority_basis: None,
        scope: Some(profile_scope()),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "filing",
            "10d:idera",
            "rows/9",
            "blake3:source-negative",
        )],
        measurements: vec![EvidenceMeasurement::Categorical(
            EvidenceCategoricalMeasurement {
                feature_id: "industry_pair".to_string(),
                value: "software_vs_biotech".to_string(),
            },
        )],
        extensions: vec![],
    }
}

fn sample_missingness() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::Missingness,
        target: EvidenceTarget::Observation {
            observation_id: "obs:sears_auto".to_string(),
            surface: "Sears Auto Center".to_string(),
            subject_hint: Some("issuer".to_string()),
        },
        operator: native_operator("anchor_presence", "1.0.0"),
        reason_code: "lei_missing".to_string(),
        policy: policy_ref(),
        authority_basis: None,
        scope: Some(profile_scope()),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "filing",
            "10d:sears_auto",
            "rows/11",
            "blake3:source-missing",
        )],
        measurements: vec![EvidenceMeasurement::Boolean(
            evidence::EvidenceBooleanMeasurement {
                feature_id: "anchor_present".to_string(),
                value: false,
            },
        )],
        extensions: vec![],
    }
}

fn sample_anti_merge_veto() -> EvidenceRecord {
    EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::AntiMergeVeto,
        target: EvidenceTarget::Pair {
            left_id: "surface:idera_inc".to_string(),
            right_id: "surface:idera_pharma".to_string(),
        },
        operator: EvidenceOperatorRef {
            namespace: "review".to_string(),
            operator_id: "cannot_link:reviewed_distinctness".to_string(),
            operator_version: "1.0.0".to_string(),
            adapter_id: Some("native".to_string()),
        },
        reason_code: "reviewed_distinctness".to_string(),
        policy: policy_ref(),
        authority_basis: Some(EvidenceAuthorityBasis::ReviewedConstraint),
        scope: Some(profile_scope()),
        temporal_scope: Some(as_of_scope("2026-06-30")),
        provenance: vec![provenance(
            "review_queue",
            "rq:idera",
            "decision/4",
            "blake3:source-veto",
        )],
        measurements: vec![
            EvidenceMeasurement::Numeric(EvidenceNumericMeasurement {
                feature_id: "score_units".to_string(),
                units: "score_units".to_string(),
                scaled_value: 10_000,
                scale: 0,
            }),
            EvidenceMeasurement::Categorical(EvidenceCategoricalMeasurement {
                feature_id: "protected_tokens".to_string(),
                value: "inc_vs_pharmaceuticals".to_string(),
            }),
        ],
        extensions: vec![],
    }
}

fn native_operator(operator_id: &str, operator_version: &str) -> EvidenceOperatorRef {
    EvidenceOperatorRef {
        namespace: "namekit".to_string(),
        operator_id: operator_id.to_string(),
        operator_version: operator_version.to_string(),
        adapter_id: Some("native".to_string()),
    }
}

fn policy_ref() -> EvidencePolicyRef {
    EvidencePolicyRef {
        policy_id: "tenant_label.default".to_string(),
        policy_version: "0.1.0".to_string(),
        content_hash: "blake3:policy".to_string(),
    }
}

fn profile_scope() -> EvidenceScope {
    EvidenceScope {
        scope_type: "profile".to_string(),
        scope_id: "cmbs_tenant_label".to_string(),
        namespace: Some("entity".to_string()),
    }
}

fn as_of_scope(as_of: &str) -> evidence::EvidenceTemporalScope {
    evidence::EvidenceTemporalScope {
        as_of: Some(as_of.to_string()),
        start_at: None,
        end_at: None,
    }
}

fn provenance(
    source_type: &str,
    source_id: &str,
    locator: &str,
    content_hash: &str,
) -> EvidenceProvenanceRef {
    EvidenceProvenanceRef {
        source_type: source_type.to_string(),
        source_id: source_id.to_string(),
        locator: locator.to_string(),
        content_hash: content_hash.to_string(),
        observed_at: Some(if source_type == "review_queue" {
            "2026-07-01".to_string()
        } else {
            "2026-06-30".to_string()
        }),
    }
}
