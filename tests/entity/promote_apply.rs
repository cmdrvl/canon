#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_DECISION_LEDGER_VERSION, CANON_ENTITY_SOLVE_VERSION, EntityArtifactHeader,
        EntityArtifactMetadata, EntityArtifactReference, EntityDeterministicSummary,
        EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
        EntityRegistrySnapshot, EntityStrategyReference,
        apply::{
            ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck, ApplyStreamRequest,
            run_apply_streaming,
        },
        artifact_chain::{
            EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage,
        },
        audit::{EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite, run_entity_audit},
        promote::{
            EntityPromoteRegistryRequest, EntityPromotedAlias, EntityPromotionAuditExpectation,
            promote_registry_aliases,
        },
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

const MANIFEST: &str = include_str!("../fixtures/entity/promote_apply/manifest.json");
const REGISTRY_BEFORE: &str =
    include_str!("../fixtures/entity/promote_apply/registry_before/registry.json");
const ALIASES_BEFORE: &str =
    include_str!("../fixtures/entity/promote_apply/registry_before/aliases.json");
const REGISTRY_AFTER: &str =
    include_str!("../fixtures/entity/promote_apply/registry_after/registry.json");
const ALIASES_AFTER: &str =
    include_str!("../fixtures/entity/promote_apply/registry_after/aliases.json");
const APPLY_INPUT: &str = include_str!("../fixtures/entity/promote_apply/apply_input.csv");
const APPLY_EXPECTED: &str = include_str!("../fixtures/entity/promote_apply/apply_expected.csv");

#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    cases: Vec<ManifestCase>,
}

#[derive(Debug, Deserialize)]
struct ManifestCase {
    id: String,
    fixture: String,
    assertion: String,
}

#[test]
fn promote_apply_fixture_manifest_has_behavioral_cases() {
    let manifest: Manifest = serde_json::from_str(MANIFEST).expect("manifest parses");
    assert_eq!(manifest.version, "canon_entity_promote_apply_fixtures.v0");
    let cases = manifest
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    for id in ["EN-PR001", "EN-PR002", "EN-A001"] {
        let case = cases.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert!(!case.fixture.trim().is_empty());
        assert!(
            !case.assertion.trim().is_empty() && !case.assertion.contains("exists"),
            "{id} must assert behavior"
        );
    }
}

#[test]
#[allow(non_snake_case)]
fn EN_PR001_stale_audit_fixture_refuses_without_mutating_registry() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_registry_before(temp.path());
    let before_registry = read_json(&temp.path().join("registry.json"));
    let before_aliases = read_json(&temp.path().join("aliases.json"));
    let audit = passing_audit();
    let mut expectation = audit_expectation(&audit);
    expectation.audit_artifact_hash = "blake3:stale-audit".to_string();

    let refusal = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: temp.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit,
        audit_expectation: expectation,
        aliases: promoted_aliases(),
        no_lint: true,
    })
    .expect_err("stale audit refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityAuditGate);
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(
        read_json(&temp.path().join("registry.json")),
        before_registry
    );
    assert_eq!(read_json(&temp.path().join("aliases.json")), before_aliases);
}

#[test]
#[allow(non_snake_case)]
fn EN_PR002_promotion_fixture_matches_expected_registry_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_registry_before(temp.path());
    let audit = passing_audit();

    let output = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: temp.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit),
        aliases: promoted_aliases(),
        no_lint: false,
    })
    .expect("promotion succeeds");

    assert_eq!(output.registry.version_after, "1.0.1");
    assert_eq!(output.registry.entry_count_after, 2);
    assert_eq!(
        read_json(&temp.path().join("registry.json")),
        serde_json::from_str::<Value>(REGISTRY_AFTER).expect("registry after")
    );
    assert_eq!(
        read_json(&temp.path().join("aliases.json")),
        serde_json::from_str::<Value>(ALIASES_AFTER).expect("aliases after")
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_A001_apply_fixture_replays_promoted_aliases_exactly() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("apply_input.csv");
    let output = temp.path().join("apply_output.csv");
    fs::write(&rows, APPLY_INPUT).expect("apply input");

    let artifact = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: ApplyRegistryReference {
            id: "cmbs-tenants".to_string(),
            version: "1.0.1".to_string(),
        },
        resolutions: &apply_resolutions(),
        safety: ApplySafetyCheck {
            expected_registry_snapshot_hash: Some("blake3:registry".to_string()),
            actual_registry_snapshot_hash: Some("blake3:registry".to_string()),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: true,
        target_rows_per_chunk: 1024,
    })
    .expect("apply succeeds");

    assert_eq!(fs::read_to_string(&rows).expect("raw input"), APPLY_INPUT);
    assert_eq!(
        fs::read_to_string(&output).expect("apply output"),
        APPLY_EXPECTED
    );
    assert_eq!(artifact.summary["resolved"], 2);
    assert_eq!(artifact.summary["unresolved"], 0);
}

fn write_registry_before(path: &Path) {
    fs::write(path.join("registry.json"), REGISTRY_BEFORE).expect("registry before");
    fs::write(path.join("aliases.json"), ALIASES_BEFORE).expect("aliases before");
}

fn promoted_aliases() -> Vec<EntityPromotedAlias> {
    vec![
        EntityPromotedAlias {
            input: "SEARS, LLC".to_string(),
            canonical_id: "TNT-SEARS".to_string(),
            canonical_type: "tenant_label".to_string(),
            rule_id: "ENTITY_REVIEW_PROMOTE".to_string(),
        },
        EntityPromotedAlias {
            input: "Kmart".to_string(),
            canonical_id: "TNT-KMART".to_string(),
            canonical_type: "tenant_label".to_string(),
            rule_id: "ENTITY_REVIEW_PROMOTE".to_string(),
        },
    ]
}

fn apply_resolutions() -> BTreeMap<String, ApplyCanonicalResolution> {
    promoted_aliases()
        .into_iter()
        .map(|alias| {
            (
                alias.input,
                ApplyCanonicalResolution {
                    canonical_id: alias.canonical_id,
                    canonical_type: alias.canonical_type,
                    rule_id: "REGISTRY_EXACT".to_string(),
                },
            )
        })
        .collect()
}

fn passing_audit() -> canon::entity::audit::EntityAuditArtifact {
    let result = solve_header();
    run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&result),
        ),
        certified_artifacts: certified_artifacts(),
        result,
        suite: passing_suite(),
    })
    .expect("audit passes")
}

fn audit_expectation(
    audit: &canon::entity::audit::EntityAuditArtifact,
) -> EntityPromotionAuditExpectation {
    EntityPromotionAuditExpectation {
        audit_artifact_hash: audit.artifact_content_hash.clone(),
        audited_artifact_hash: "blake3:solve".to_string(),
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        required_gate_ids: vec!["G14".to_string()],
    }
}

fn solve_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        metadata: metadata(),
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([("entity_count".to_string(), 2)]),
            labels: BTreeMap::new(),
        },
    }
}

fn metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "tenant_label".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            canonical_type: "tenant_label".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "cmbs_tenant_label.aliases".to_string(),
                distinct: "cmbs_tenant_label.distinct".to_string(),
                relations: "cmbs_tenant_label.relations".to_string(),
            },
            content_hash: Some("blake3:profile".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "cmbs_tenant_label.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "cmbs-tenants".to_string(),
            version: "2026.06.25".to_string(),
            source: "registries/cmbs-tenants".to_string(),
            lookup_snapshot_hash: "blake3:registry".to_string(),
            sidecar_snapshot_hash: Some("blake3:sidecars".to_string()),
        },
        patch_namespace: "cmbs_tenant_label.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 2,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![],
        patch_set: None,
        namekit: None,
        artifact_content_hash: "blake3:solve".to_string(),
    }
}

fn certified_artifacts() -> Vec<EntityArtifactReference> {
    vec![
        EntityArtifactReference {
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            content_hash: "blake3:solve".to_string(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
            content_hash: "blake3:review-queue".to_string(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_DECISION_LEDGER_VERSION.to_string(),
            content_hash: "blake3:decision-ledger".to_string(),
        },
    ]
}

fn passing_suite() -> EntityAuditSuite {
    EntityAuditSuite {
        id: "promote_apply_fixture".to_string(),
        version: "2026.06.26".to_string(),
        gates: vec![
            EntityAuditGateCheck {
                gate_id: "G01".to_string(),
                label: "artifact continuity".to_string(),
                passed: true,
                expected: "all_hashes_match".to_string(),
                actual: "all_hashes_match".to_string(),
                evidence: BTreeMap::new(),
            },
            EntityAuditGateCheck {
                gate_id: "G09".to_string(),
                label: "decision ledger continuity".to_string(),
                passed: true,
                expected: "continuous_jsonl".to_string(),
                actual: "continuous_jsonl".to_string(),
                evidence: BTreeMap::new(),
            },
            EntityAuditGateCheck {
                gate_id: "G14".to_string(),
                label: "promotion gate".to_string(),
                passed: true,
                expected: "audit_status=passed".to_string(),
                actual: "audit_status=passed".to_string(),
                evidence: BTreeMap::new(),
            },
        ],
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}
