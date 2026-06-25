use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityInputReference, EntityPatchSetReference,
        EntityRegistrySnapshot, EntityStrategyReference,
        profile::{EntityProfileDocument, EntityProfileError, EntityProfileFirewall},
    },
};

#[test]
fn profile_firewall_contract_artifact_metadata_carries_scope_fields() {
    let profile = cmbs_profile();
    let firewall = EntityProfileFirewall::new(
        &profile,
        "blake3:strategy",
        "blake3:registry",
        profile.patch_namespaces.aliases.clone(),
    )
    .expect("complete firewall metadata validates");

    let metadata = EntityArtifactMetadata {
        profile: firewall.profile.clone(),
        strategy: EntityStrategyReference {
            id: "cmbs_tenant_label.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: firewall.strategy_content_hash.clone(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "cmbs-tenants".to_string(),
            version: "2026.06.25".to_string(),
            source: "registries/cmbs-tenants".to_string(),
            lookup_snapshot_hash: firewall.registry_snapshot_hash.clone(),
            sidecar_snapshot_hash: None,
        },
        patch_namespace: firewall.patch_namespace.clone(),
        input: Some(EntityInputReference {
            row_count: 3,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![],
        patch_set: Some(EntityPatchSetReference {
            content_hash: "blake3:patch".to_string(),
            paths: vec!["patches/cmbs-tenants.yaml".to_string()],
        }),
        namekit: None,
        artifact_content_hash: "blake3:artifact".to_string(),
    };

    let value = serde_json::to_value(metadata).expect("metadata serializes");
    assert_eq!(value["profile"]["id"], "cmbs_tenant_label");
    assert_eq!(value["profile"]["version"], "0.1.0");
    assert_eq!(value["profile"]["entity_type"], "tenant_label");
    assert_eq!(
        value["profile"]["identity_semantics"],
        "canonical_display_label"
    );
    assert_eq!(value["profile"]["canonical_type"], "tenant_label");
    assert_eq!(value["strategy"]["content_hash"], "blake3:strategy");
    assert_eq!(
        value["registry_snapshot"]["lookup_snapshot_hash"],
        "blake3:registry"
    );
    assert_eq!(value["patch_namespace"], "cmbs_tenant_label.aliases");
}

#[test]
fn profile_firewall_contract_rejects_incomplete_artifact_scope() {
    let profile = cmbs_profile();
    let error =
        EntityProfileFirewall::new(&profile, "", "blake3:registry", "cmbs_tenant_label.aliases")
            .expect_err("missing strategy hash refuses");

    assert_entity_error_code(&error, RefusalCode::EEntityArtifactContract);
    assert!(
        error.detail["missing"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("strategy_content_hash"))
    );
}

#[test]
fn profile_firewall_contract_allows_same_scope_same_as_reuse() {
    let profile = cmbs_profile();
    let source = EntityProfileFirewall::new(
        &profile,
        "blake3:strategy-a",
        "blake3:registry-a",
        profile.patch_namespaces.aliases.clone(),
    )
    .expect("source scope validates");
    let target = EntityProfileFirewall::new(
        &profile,
        "blake3:strategy-b",
        "blake3:registry-b",
        profile.patch_namespaces.aliases.clone(),
    )
    .expect("target scope validates");

    source
        .validate_same_as_reuse(&target)
        .expect("same profile identity semantics can reuse same-as evidence");
}

#[test]
fn cross_profile_same_as_refusal_blocks_merge_reuse() {
    let cmbs = cmbs_profile();
    let regab = regab_profile();
    let source = EntityProfileFirewall::new(
        &cmbs,
        "blake3:cmbs-strategy",
        "blake3:cmbs-registry",
        cmbs.patch_namespaces.aliases.clone(),
    )
    .expect("cmbs scope validates");
    let target = EntityProfileFirewall::new(
        &regab,
        "blake3:regab-strategy",
        "blake3:regab-registry",
        regab.patch_namespaces.aliases.clone(),
    )
    .expect("regab scope validates");

    let error = source
        .validate_same_as_reuse(&target)
        .expect_err("cross-profile same-as reuse refuses");

    assert_entity_error_code(&error, RefusalCode::EEntityProfile);
    assert_eq!(error.detail["mode"], "same_as");
    let mismatches = error.detail["mismatches"].as_array().unwrap();
    assert!(mismatches.iter().any(|entry| entry["field"] == "profile"));
    assert!(
        mismatches
            .iter()
            .any(|entry| entry["field"] == "identity_semantics")
    );
}

#[test]
fn profile_firewall_contract_relation_handoff_never_authorizes_merge() {
    let cmbs = cmbs_profile();
    let regab = regab_profile();
    let source = EntityProfileFirewall::new(
        &cmbs,
        "blake3:cmbs-strategy",
        "blake3:cmbs-registry",
        cmbs.patch_namespaces.relations.clone(),
    )
    .expect("cmbs relation scope validates");
    let target = EntityProfileFirewall::new(
        &regab,
        "blake3:regab-strategy",
        "blake3:regab-registry",
        regab.patch_namespaces.relations.clone(),
    )
    .expect("regab relation scope validates");

    let handoff = source
        .relation_handoff(&target, "cross_profile_alignment")
        .expect("cross-profile alignment can be handed off as relation");

    assert_eq!(handoff.relation, "cross_profile_alignment");
    assert_eq!(handoff.source_profile.id, "cmbs_tenant_label");
    assert_eq!(handoff.target_profile.id, "regab_firm_identity");
    assert!(!handoff.merge_authorized);

    let error = source
        .relation_handoff(&target, "same_as")
        .expect_err("relation handoff does not encode same-as");
    assert_entity_error_code(&error, RefusalCode::EEntityProfile);
}

fn cmbs_profile() -> EntityProfileDocument {
    EntityProfileDocument::from_yaml_str(include_str!(
        "fixtures/entity/profiles/cmbs_tenant_label.yaml"
    ))
    .expect("cmbs profile validates")
}

fn regab_profile() -> EntityProfileDocument {
    EntityProfileDocument::from_yaml_str(include_str!(
        "fixtures/entity/profiles/regab_firm_identity.yaml"
    ))
    .expect("regab profile validates")
}

fn assert_entity_error_code(error: &EntityProfileError, expected: RefusalCode) {
    let refusal = error.to_refusal();
    assert_eq!(refusal.code, expected);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
}
