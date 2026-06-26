#![forbid(unsafe_code)]

use assert_cmd::Command;
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
        profiles::cmbs::{
            CMBS_TENANT_CANONICAL_TYPE, CMBS_TENANT_IDENTITY_SEMANTICS, CMBS_TENANT_PROFILE_ID,
            CMBS_TENANT_PROFILE_VERSION, CmbsTenantIdAllocation, CmbsTenantIdAllocationRequest,
            CmbsTenantIdAllocator,
        },
        promote::{
            EntityPromoteRegistryRequest, EntityPromotedAlias, EntityPromotionAuditExpectation,
            promote_registry_aliases,
        },
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

#[test]
fn cmbs_tenant_id_exact_replay_promotes_tnt_ids_and_applies_exact_registry() {
    let allocation = allocate_sears_tenant_id("blake3:registry");
    assert_eq!(allocation.canonical_id, "TNT-SEARS");

    let registry = make_registry("1.0.0", json!([]), None);
    let audit = passing_audit("blake3:registry");
    let alias = promoted_alias_from_allocation(&allocation, "Sears");
    let promotion = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit, "blake3:registry"),
        aliases: vec![alias.clone()],
        no_lint: false,
    })
    .expect("CMBS tenant promotion succeeds");

    assert_eq!(promotion.registry.version_after, "1.0.1");
    assert_eq!(promotion.registry.entry_count_after, 1);
    assert_eq!(promotion.aliases, vec![alias.clone()]);

    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(
        registry_json["entity_profile"]["id"],
        CMBS_TENANT_PROFILE_ID
    );
    assert_eq!(
        registry_json["entity_profile"]["version"],
        CMBS_TENANT_PROFILE_VERSION
    );
    assert_eq!(
        registry_json["entity_profile"]["identity_semantics"],
        CMBS_TENANT_IDENTITY_SEMANTICS
    );
    assert_eq!(
        registry_json["entity_profile"]["canonical_type"],
        CMBS_TENANT_CANONICAL_TYPE
    );
    assert_eq!(
        registry_json["entity_profile"]["patch_namespaces"]["aliases"],
        "cmbs_tenant_label.aliases"
    );

    let exact_lookup_input = registry.path().join("lookup.csv");
    fs::write(&exact_lookup_input, "tenant_name\nSears\n").expect("lookup csv");
    let exact_lookup = canon_command()
        .args([
            exact_lookup_input.to_str().expect("lookup path"),
            "--registry",
            registry.path().to_str().expect("registry path"),
            "--column",
            "tenant_name",
            "--no-witness",
            "--explicit",
        ])
        .assert()
        .success();
    let lookup_payload: Value = serde_json::from_slice(exact_lookup.get_output().stdout.as_slice())
        .expect("exact lookup json");
    assert_eq!(lookup_payload["outcome"], "RESOLVED");
    assert_eq!(
        lookup_payload["mappings"][0]["canonical_id"],
        "u8:TNT-SEARS"
    );

    let rows = registry.path().join("tenants.csv");
    let output = registry.path().join("tenants.canon.csv");
    let raw_rows = concat!(
        "loan_id,tenant_name,row_order\n",
        "L-001,Sears,1\n",
        "L-9482,Sears,9482\n",
    );
    fs::write(&rows, raw_rows).expect("rows");
    let resolutions = BTreeMap::from([(
        alias.input.clone(),
        ApplyCanonicalResolution {
            canonical_id: alias.canonical_id.clone(),
            canonical_type: alias.canonical_type.clone(),
            rule_id: "REGISTRY_EXACT".to_string(),
        },
    )]);
    let artifact = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: ApplyRegistryReference {
            id: "cmbs-tenants".to_string(),
            version: "1.0.1".to_string(),
        },
        resolutions: &resolutions,
        safety: ApplySafetyCheck {
            expected_profile_id: Some(CMBS_TENANT_PROFILE_ID.to_string()),
            actual_profile_id: Some(
                registry_json["entity_profile"]["id"]
                    .as_str()
                    .expect("profile id")
                    .to_string(),
            ),
            expected_identity_semantics: Some(CMBS_TENANT_IDENTITY_SEMANTICS.to_string()),
            actual_identity_semantics: Some(
                registry_json["entity_profile"]["identity_semantics"]
                    .as_str()
                    .expect("identity semantics")
                    .to_string(),
            ),
            expected_registry_snapshot_hash: Some("blake3:registry".to_string()),
            actual_registry_snapshot_hash: Some("blake3:registry".to_string()),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: true,
        target_rows_per_chunk: 1024,
    })
    .expect("apply exact replay succeeds");

    assert_eq!(fs::read_to_string(&rows).expect("raw rows"), raw_rows);
    assert_eq!(artifact.summary["rows"], 2);
    assert_eq!(artifact.summary["resolved"], 2);
    assert_eq!(artifact.summary["unresolved"], 0);
    assert_eq!(
        fs::read_to_string(&output).expect("apply output"),
        concat!(
            "loan_id,tenant_name,row_order,canonical_id,canonical_type,",
            "canonical_status,canonical_registry_id,canonical_registry_version,canonical_rule_id\n",
            "L-001,Sears,1,TNT-SEARS,tenant_label,resolved,cmbs-tenants,1.0.1,REGISTRY_EXACT\n",
            "L-9482,Sears,9482,TNT-SEARS,tenant_label,resolved,cmbs-tenants,1.0.1,REGISTRY_EXACT\n",
        )
    );
}

#[test]
fn promote_profile_mismatch_refusal_preserves_profiled_registry() {
    let wrong_profile = json!({
        "id": "regab_firm_identity",
        "version": "0.1.0",
        "entity_type": "organization",
        "identity_semantics": "same_firm_or_reviewed_alias",
        "canonical_type": "org",
        "patch_namespaces": {
            "aliases": "regab_firm_identity.aliases",
            "distinct": "regab_firm_identity.distinct",
            "relations": "regab_firm_identity.relations"
        }
    });
    let registry = make_registry("1.0.0", json!([]), Some(wrong_profile));
    let registry_before =
        fs::read_to_string(registry.path().join("registry.json")).expect("registry before");
    let aliases_before =
        fs::read_to_string(registry.path().join("aliases.json")).expect("aliases before");
    let audit = passing_audit("blake3:registry");
    let allocation = allocate_sears_tenant_id("blake3:registry");

    let refusal = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit, "blake3:registry"),
        aliases: vec![promoted_alias_from_allocation(&allocation, "Sears")],
        no_lint: true,
    })
    .expect_err("profiled Reg AB registry refuses CMBS promotion");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "promote");
    assert_eq!(refusal.detail["field"], "id");
    assert_eq!(refusal.detail["expected"], CMBS_TENANT_PROFILE_ID);
    assert_eq!(refusal.detail["actual"], "regab_firm_identity");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| text.contains("entity_profile"))
    );
    assert_eq!(
        fs::read_to_string(registry.path().join("registry.json")).expect("registry after"),
        registry_before
    );
    assert_eq!(
        fs::read_to_string(registry.path().join("aliases.json")).expect("aliases after"),
        aliases_before
    );
}

#[test]
fn apply_registry_snapshot_mismatch_refusal_preserves_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let output = temp.path().join("rows.canon.csv");
    fs::write(&rows, "loan_id,tenant_name\nL-001,Sears\n").expect("rows");
    fs::write(&output, "sentinel output\n").expect("sentinel output");
    let output_before = fs::read_to_string(&output).expect("output before");
    let resolutions = BTreeMap::from([(
        "Sears".to_string(),
        ApplyCanonicalResolution {
            canonical_id: "TNT-SEARS".to_string(),
            canonical_type: CMBS_TENANT_CANONICAL_TYPE.to_string(),
            rule_id: "REGISTRY_EXACT".to_string(),
        },
    )]);

    let refusal = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: ApplyRegistryReference {
            id: "cmbs-tenants".to_string(),
            version: "1.0.1".to_string(),
        },
        resolutions: &resolutions,
        safety: ApplySafetyCheck {
            expected_profile_id: Some(CMBS_TENANT_PROFILE_ID.to_string()),
            actual_profile_id: Some(CMBS_TENANT_PROFILE_ID.to_string()),
            expected_identity_semantics: Some(CMBS_TENANT_IDENTITY_SEMANTICS.to_string()),
            actual_identity_semantics: Some(CMBS_TENANT_IDENTITY_SEMANTICS.to_string()),
            expected_registry_snapshot_hash: Some("blake3:registry-before".to_string()),
            actual_registry_snapshot_hash: Some("blake3:registry-after".to_string()),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: true,
        target_rows_per_chunk: 1024,
    })
    .expect_err("stale registry snapshot refuses before output write");

    assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
    assert_eq!(refusal.detail["stage"], "apply");
    assert_eq!(refusal.detail["field"], "registry_snapshot_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(
        fs::read_to_string(&output).expect("output after"),
        output_before
    );
}

fn allocate_sears_tenant_id(registry_snapshot_hash: &str) -> CmbsTenantIdAllocation {
    CmbsTenantIdAllocator::default()
        .allocate(&CmbsTenantIdAllocationRequest::new(
            "Sears",
            "sears",
            registry_snapshot_hash,
            "blake3:cmbs-alias-patch-001",
            "review:cmbs:sears:001",
        ))
        .expect("tenant id allocation succeeds")
}

fn promoted_alias_from_allocation(
    allocation: &CmbsTenantIdAllocation,
    input: &str,
) -> EntityPromotedAlias {
    EntityPromotedAlias {
        input: input.to_string(),
        canonical_id: allocation.canonical_id.clone(),
        canonical_type: allocation.profile.canonical_type.clone(),
        rule_id: "ENTITY_REVIEW_PROMOTE".to_string(),
    }
}

fn passing_audit(registry_snapshot_hash: &str) -> canon::entity::audit::EntityAuditArtifact {
    let result = solve_header(registry_snapshot_hash);
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
    registry_snapshot_hash: &str,
) -> EntityPromotionAuditExpectation {
    EntityPromotionAuditExpectation {
        audit_artifact_hash: audit.artifact_content_hash.clone(),
        audited_artifact_hash: "blake3:solve".to_string(),
        profile_id: CMBS_TENANT_PROFILE_ID.to_string(),
        profile_version: CMBS_TENANT_PROFILE_VERSION.to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: registry_snapshot_hash.to_string(),
        required_gate_ids: vec!["G14".to_string()],
    }
}

fn solve_header(registry_snapshot_hash: &str) -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        metadata: metadata(registry_snapshot_hash),
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([("entity_count".to_string(), 1)]),
            labels: BTreeMap::new(),
        },
    }
}

fn metadata(registry_snapshot_hash: &str) -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: CMBS_TENANT_PROFILE_ID.to_string(),
            version: CMBS_TENANT_PROFILE_VERSION.to_string(),
            entity_type: "tenant_label".to_string(),
            identity_semantics: CMBS_TENANT_IDENTITY_SEMANTICS.to_string(),
            canonical_type: CMBS_TENANT_CANONICAL_TYPE.to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "cmbs_tenant_label.aliases".to_string(),
                distinct: "cmbs_tenant_label.distinct".to_string(),
                relations: "cmbs_tenant_label.relations".to_string(),
            },
            content_hash: Some("blake3:cmbs-profile".to_string()),
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
            lookup_snapshot_hash: registry_snapshot_hash.to_string(),
            sidecar_snapshot_hash: Some("blake3:sidecars".to_string()),
        },
        patch_namespace: "cmbs_tenant_label.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 1,
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
        id: "cmbs_tenant_id_replay".to_string(),
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

fn make_registry(version: &str, entries: Value, entity_profile: Option<Value>) -> TempDir {
    let temp = TempDir::new().expect("tempdir");
    let entry_count = entries.as_array().expect("entries array").len();
    let mut registry = json!({
        "id": "cmbs-tenants",
        "version": version,
        "description": "CMBS tenant replay fixture registry",
        "updated": "2026-06-26",
        "entry_count": entry_count,
        "owner": "test-suite"
    });
    if let Some(entity_profile) = entity_profile {
        registry["entity_profile"] = entity_profile;
    }
    fs::write(
        temp.path().join("registry.json"),
        serde_json::to_vec_pretty(&registry).expect("registry json"),
    )
    .expect("write registry");
    fs::write(
        temp.path().join("aliases.json"),
        serde_json::to_vec_pretty(&entries).expect("aliases json"),
    )
    .expect("write aliases");
    temp
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}
