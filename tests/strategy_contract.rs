#![forbid(unsafe_code)]

#[path = "../src/strategy/types.rs"]
mod strategy_types;

use serde_json::{Value, json};
use std::collections::BTreeSet;
use strategy_types::{
    LegacyStrategyFootprint, StrategyAllowedInput, StrategyAuditFixtureKind,
    StrategyCapabilityRequirement, StrategyCompatibility, StrategyCompatibilityKind,
    StrategyDefinition, StrategyDoctrineErrorCode, StrategyExecutionMode, StrategyExecutionPolicy,
    StrategyKind, StrategyOutputKind, StrategyPromotionSemantics, StrategyPromotionTarget,
    StrategySelectionKey, classify_legacy_footprint, strategy_schema_version,
};

const STRATEGY_SCHEMA_JSON: &str = include_str!("../schemas/canon.strategy.v1.schema.json");
const ARCHITECTURE_DOC: &str = include_str!("../docs/IDENTITY_ARCHITECTURE.md");

#[test]
fn strategy_schema_declares_typed_kinds_and_lookup_boundary() {
    let schema: Value = serde_json::from_str(STRATEGY_SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], "canon.strategy.v1");
    assert_eq!(
        schema["properties"]["version"]["const"],
        "canon.strategy.v1"
    );

    let kinds = schema["x-canon-contract"]["typed_kinds"]
        .as_array()
        .expect("typed kinds array")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        kinds,
        BTreeSet::from([
            "identity-evidence",
            "record-linkage",
            "schema-transform",
            "task-transform"
        ])
    );
    assert!(
        schema["x-canon-contract"]["exact_lookup_boundary"]
            .as_str()
            .unwrap()
            .contains("never")
    );
}

#[test]
fn doctrine_document_names_canonical_procedural_strategy_boundary() {
    assert!(ARCHITECTURE_DOC.contains("canonical procedural knowledge"));
    assert!(ARCHITECTURE_DOC.contains("identity-evidence"));
    assert!(ARCHITECTURE_DOC.contains("record-linkage"));
    assert!(ARCHITECTURE_DOC.contains("schema-transform"));
    assert!(ARCHITECTURE_DOC.contains("task-transform"));
    assert!(ARCHITECTURE_DOC.contains("never part of exact lookup"));
}

#[test]
fn every_typed_kind_round_trips_and_validates() {
    for fixture in typed_fixtures() {
        let parsed: StrategyDefinition =
            serde_json::from_value(fixture.clone()).expect("fixture deserializes");
        parsed.validate().expect("fixture validates");

        let round_tripped = serde_json::to_value(&parsed).expect("fixture serializes");
        assert_eq!(round_tripped, fixture);
    }
}

#[test]
fn unknown_kind_and_incompatible_fields_are_rejected() {
    let unknown = serde_json::from_value::<StrategyDefinition>(json!({
        "version": "canon.strategy.v1",
        "kind": "unknown-kind",
        "selection_key": {"type": "schema-transform", "schema_fingerprint": "blake3:schema", "skill_hash": "blake3:skill"},
        "allowed_inputs": [{"type": "schema-profile", "schema_source": "canon_strategy_profile.v0"}],
        "declared_outputs": ["frozen-script-pointer"],
        "capability_requirements": ["deterministic-local-execution", "no-live-network", "pinned-dependencies", "audit-fixtures-required", "exact-lookup-boundary"],
        "execution_policy": {"mode": "selection-only", "deterministic_replay": true, "exact_lookup_phase": false, "permits_live_network": false, "requires_pinned_dependencies": true},
        "audit_fixtures": ["deterministic-stdout-suite"],
        "compatibility": {"type": "schema-tiered", "relation": "same-columns-types-cardinality-tiers"},
        "promotion": {"target": "strategy-registry-champion", "requires_version_bump": true, "requires_audit": true, "allows_operator_attestation": true, "requires_review_gate": false}
    }));
    assert!(unknown.is_err());

    let incompatible: StrategyDefinition = serde_json::from_value(json!({
        "version": "canon.strategy.v1",
        "kind": "schema-transform",
        "selection_key": {"type": "schema-transform", "schema_fingerprint": "blake3:schema", "skill_hash": "blake3:skill"},
        "allowed_inputs": [{"type": "schema-profile", "schema_source": "canon_strategy_profile.v0"}],
        "declared_outputs": ["frozen-script-pointer"],
        "capability_requirements": ["deterministic-local-execution", "no-live-network", "pinned-dependencies", "audit-fixtures-required", "exact-lookup-boundary"],
        "execution_policy": {"mode": "selection-only", "deterministic_replay": true, "exact_lookup_phase": true, "permits_live_network": false, "requires_pinned_dependencies": true},
        "audit_fixtures": ["deterministic-stdout-suite"],
        "compatibility": {"type": "schema-tiered", "relation": "same-columns-types-cardinality-tiers"},
        "promotion": {"target": "strategy-registry-champion", "requires_version_bump": true, "requires_audit": true, "allows_operator_attestation": true, "requires_review_gate": false}
    }))
    .expect("incompatible fixture still deserializes");
    let error = incompatible
        .validate()
        .expect_err("validate rejects lookup-phase execution");
    assert_eq!(error.code, StrategyDoctrineErrorCode::IncompatibleFields);
}

#[test]
fn legacy_schema_and_task_surfaces_map_to_exactly_one_kind() {
    let schema_kind = classify_legacy_footprint(&LegacyStrategyFootprint {
        schema_key: true,
        task_key: false,
        profile_scope: false,
        linkage_scope: false,
        frozen_script_pointer: true,
        evidence_bundle: false,
        linkage_bundle: false,
        registry_knowledge_proposal: false,
    })
    .expect("schema surface maps");
    assert_eq!(schema_kind, StrategyKind::SchemaTransform);

    let task_kind = classify_legacy_footprint(&LegacyStrategyFootprint {
        schema_key: false,
        task_key: true,
        profile_scope: false,
        linkage_scope: false,
        frozen_script_pointer: true,
        evidence_bundle: false,
        linkage_bundle: false,
        registry_knowledge_proposal: false,
    })
    .expect("task surface maps");
    assert_eq!(task_kind, StrategyKind::TaskTransform);
}

#[test]
fn mixed_kind_ambiguity_is_rejected_deterministically() {
    let ambiguous = classify_legacy_footprint(&LegacyStrategyFootprint {
        schema_key: true,
        task_key: true,
        profile_scope: false,
        linkage_scope: false,
        frozen_script_pointer: true,
        evidence_bundle: false,
        linkage_bundle: false,
        registry_knowledge_proposal: false,
    })
    .expect_err("mixed schema/task footprint rejects");
    assert_eq!(
        ambiguous.code,
        StrategyDoctrineErrorCode::AmbiguousMigration
    );

    let selection = typed_schema_transform();
    assert_eq!(
        selection.selection_summary(),
        "select strategy kind=schema-transform key=schema=blake3:schema-fingerprint skill=blake3:skill-schema mode=selection-only exact_lookup=never"
    );
    assert_eq!(
        selection.explain_summary(),
        "strategy kind=schema-transform outputs=[frozen-script-pointer] compatibility=schema-tiered promotion=strategy-registry-champion"
    );

    let task = typed_task_transform();
    assert_eq!(
        task.selection_summary(),
        "select strategy kind=task-transform key=task=normalize_vendor_extract skill=blake3:skill-task mode=selection-only exact_lookup=never"
    );
}

#[test]
fn canonical_kind_catalog_is_complete_and_stable() {
    let kinds = StrategyKind::all()
        .into_iter()
        .map(|kind| {
            serde_json::to_value(kind)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            "identity-evidence".to_string(),
            "record-linkage".to_string(),
            "schema-transform".to_string(),
            "task-transform".to_string()
        ]
    );
    assert_eq!(strategy_schema_version(), "canon.strategy.v1");
}

fn typed_fixtures() -> Vec<Value> {
    vec![
        serde_json::to_value(typed_identity_evidence()).unwrap(),
        serde_json::to_value(typed_record_linkage()).unwrap(),
        serde_json::to_value(typed_schema_transform()).unwrap(),
        serde_json::to_value(typed_task_transform()).unwrap(),
    ]
}

fn typed_identity_evidence() -> StrategyDefinition {
    StrategyDefinition {
        version: strategy_schema_version().to_string(),
        kind: StrategyKind::IdentityEvidence,
        selection_key: StrategySelectionKey::IdentityEvidence {
            profile_id: "regab_firm_identity".to_string(),
            skill_hash: "blake3:skill-identity".to_string(),
        },
        allowed_inputs: vec![StrategyAllowedInput::ProfiledObservations {
            profile_id: "regab_firm_identity".to_string(),
        }],
        declared_outputs: vec![
            StrategyOutputKind::EvidenceBundle,
            StrategyOutputKind::RegistryKnowledgeProposal,
        ],
        capability_requirements: vec![
            StrategyCapabilityRequirement::DeterministicLocalExecution,
            StrategyCapabilityRequirement::NoLiveNetwork,
            StrategyCapabilityRequirement::AuditFixturesRequired,
            StrategyCapabilityRequirement::ExactLookupBoundary,
            StrategyCapabilityRequirement::ReviewGateForRegistryMutation,
        ],
        execution_policy: StrategyExecutionPolicy {
            mode: StrategyExecutionMode::WorkbenchExecution,
            deterministic_replay: true,
            exact_lookup_phase: false,
            permits_live_network: false,
            requires_pinned_dependencies: false,
        },
        audit_fixtures: vec![
            StrategyAuditFixtureKind::HoldoutPairs,
            StrategyAuditFixtureKind::HardNegatives,
            StrategyAuditFixtureKind::ReviewQueue,
        ],
        compatibility: StrategyCompatibility {
            kind: StrategyCompatibilityKind::ProfileScoped,
            relation: "same-profile-and-skill-hash".to_string(),
        },
        promotion: StrategyPromotionSemantics {
            target: StrategyPromotionTarget::RegistryKnowledgePromotion,
            requires_version_bump: true,
            requires_audit: true,
            allows_operator_attestation: false,
            requires_review_gate: true,
        },
    }
}

fn typed_record_linkage() -> StrategyDefinition {
    StrategyDefinition {
        version: strategy_schema_version().to_string(),
        kind: StrategyKind::RecordLinkage,
        selection_key: StrategySelectionKey::RecordLinkage {
            linkage_map_id: "cmbs-loan-linkage.v1".to_string(),
            skill_hash: "blake3:skill-linkage".to_string(),
        },
        allowed_inputs: vec![StrategyAllowedInput::TwoTapeRecords {
            linkage_map_id: "cmbs-loan-linkage.v1".to_string(),
        }],
        declared_outputs: vec![
            StrategyOutputKind::LinkageBundle,
            StrategyOutputKind::RegistryKnowledgeProposal,
        ],
        capability_requirements: vec![
            StrategyCapabilityRequirement::DeterministicLocalExecution,
            StrategyCapabilityRequirement::NoLiveNetwork,
            StrategyCapabilityRequirement::AuditFixturesRequired,
            StrategyCapabilityRequirement::ExactLookupBoundary,
            StrategyCapabilityRequirement::ReviewGateForRegistryMutation,
        ],
        execution_policy: StrategyExecutionPolicy {
            mode: StrategyExecutionMode::WorkbenchExecution,
            deterministic_replay: true,
            exact_lookup_phase: false,
            permits_live_network: false,
            requires_pinned_dependencies: false,
        },
        audit_fixtures: vec![
            StrategyAuditFixtureKind::HoldoutPairs,
            StrategyAuditFixtureKind::LinkageGold,
        ],
        compatibility: StrategyCompatibility {
            kind: StrategyCompatibilityKind::FieldMapScoped,
            relation: "same-linkage-map-and-skill-hash".to_string(),
        },
        promotion: StrategyPromotionSemantics {
            target: StrategyPromotionTarget::RegistryKnowledgePromotion,
            requires_version_bump: true,
            requires_audit: true,
            allows_operator_attestation: false,
            requires_review_gate: true,
        },
    }
}

fn typed_schema_transform() -> StrategyDefinition {
    StrategyDefinition {
        version: strategy_schema_version().to_string(),
        kind: StrategyKind::SchemaTransform,
        selection_key: StrategySelectionKey::SchemaTransform {
            schema_fingerprint: "blake3:schema-fingerprint".to_string(),
            skill_hash: "blake3:skill-schema".to_string(),
        },
        allowed_inputs: vec![StrategyAllowedInput::SchemaProfile {
            schema_source: "canon_strategy_profile.v0".to_string(),
        }],
        declared_outputs: vec![StrategyOutputKind::FrozenScriptPointer],
        capability_requirements: vec![
            StrategyCapabilityRequirement::DeterministicLocalExecution,
            StrategyCapabilityRequirement::NoLiveNetwork,
            StrategyCapabilityRequirement::PinnedDependencies,
            StrategyCapabilityRequirement::AuditFixturesRequired,
            StrategyCapabilityRequirement::ExactLookupBoundary,
        ],
        execution_policy: StrategyExecutionPolicy {
            mode: StrategyExecutionMode::SelectionOnly,
            deterministic_replay: true,
            exact_lookup_phase: false,
            permits_live_network: false,
            requires_pinned_dependencies: true,
        },
        audit_fixtures: vec![StrategyAuditFixtureKind::DeterministicStdoutSuite],
        compatibility: StrategyCompatibility {
            kind: StrategyCompatibilityKind::SchemaTiered,
            relation: "exact-compatible-partial-unresolved schema tiers".to_string(),
        },
        promotion: StrategyPromotionSemantics {
            target: StrategyPromotionTarget::StrategyRegistryChampion,
            requires_version_bump: true,
            requires_audit: true,
            allows_operator_attestation: true,
            requires_review_gate: false,
        },
    }
}

fn typed_task_transform() -> StrategyDefinition {
    StrategyDefinition {
        version: strategy_schema_version().to_string(),
        kind: StrategyKind::TaskTransform,
        selection_key: StrategySelectionKey::TaskTransform {
            task: "normalize_vendor_extract".to_string(),
            skill_hash: "blake3:skill-task".to_string(),
        },
        allowed_inputs: vec![StrategyAllowedInput::ExactTask {
            task: "normalize_vendor_extract".to_string(),
        }],
        declared_outputs: vec![StrategyOutputKind::FrozenScriptPointer],
        capability_requirements: vec![
            StrategyCapabilityRequirement::DeterministicLocalExecution,
            StrategyCapabilityRequirement::NoLiveNetwork,
            StrategyCapabilityRequirement::PinnedDependencies,
            StrategyCapabilityRequirement::AuditFixturesRequired,
            StrategyCapabilityRequirement::ExactLookupBoundary,
        ],
        execution_policy: StrategyExecutionPolicy {
            mode: StrategyExecutionMode::SelectionOnly,
            deterministic_replay: true,
            exact_lookup_phase: false,
            permits_live_network: false,
            requires_pinned_dependencies: true,
        },
        audit_fixtures: vec![StrategyAuditFixtureKind::DeterministicStdoutSuite],
        compatibility: StrategyCompatibility {
            kind: StrategyCompatibilityKind::TaskExactOnly,
            relation: "exact-task-key-and-skill-hash only".to_string(),
        },
        promotion: StrategyPromotionSemantics {
            target: StrategyPromotionTarget::StrategyRegistryChampion,
            requires_version_bump: true,
            requires_audit: true,
            allows_operator_attestation: true,
            requires_review_gate: false,
        },
    }
}
