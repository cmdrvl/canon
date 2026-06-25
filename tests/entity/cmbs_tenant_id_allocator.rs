use canon::{
    RefusalCode,
    entity::profiles::cmbs::{
        CMBS_TENANT_CANONICAL_TYPE, CMBS_TENANT_ID_ALLOCATOR_VERSION,
        CMBS_TENANT_IDENTITY_SEMANTICS, CMBS_TENANT_PROFILE_ID, CMBS_TENANT_PROFILE_VERSION,
        CmbsTenantIdAllocationRequest, CmbsTenantIdAllocator, CmbsTenantReservedId,
        candidate_tnt_id, is_valid_tnt_id,
    },
};

#[test]
fn cmbs_tenant_id_allocator_derives_deterministic_tnt_ids_with_profile_metadata() {
    let allocator = CmbsTenantIdAllocator::default();
    let request = sears_request("blake3:cmbs-registry-20260625");

    let first = allocator
        .allocate(&request)
        .expect("first allocation succeeds");
    let second = allocator
        .allocate(&request)
        .expect("second allocation is deterministic");

    assert_eq!(first, second);
    assert_eq!(first.version, CMBS_TENANT_ID_ALLOCATOR_VERSION);
    assert_eq!(first.canonical_id, "TNT-SEARS");
    assert!(is_valid_tnt_id(&first.canonical_id));
    assert_eq!(first.profile.id, CMBS_TENANT_PROFILE_ID);
    assert_eq!(first.profile.version, CMBS_TENANT_PROFILE_VERSION);
    assert_eq!(first.profile.canonical_type, CMBS_TENANT_CANONICAL_TYPE);
    assert_eq!(
        first.profile.identity_semantics,
        CMBS_TENANT_IDENTITY_SEMANTICS
    );
    assert_eq!(first.candidate_source, "reviewed_display_label");
    assert_eq!(first.candidate_normalization, "uppercase_ascii_slug");
    assert_eq!(
        first.collision_policy,
        "refuse_without_suffix_or_silent_remint"
    );
    assert_eq!(first.side_effects.registry_writes, 0);
    assert_eq!(first.side_effects.output_rows_written, 0);
}

#[test]
fn cmbs_tenant_id_allocator_uses_uppercase_ascii_slug_shape() {
    for (label, expected) in [
        ("24 Hour Fitness", "TNT-24-HOUR-FITNESS"),
        ("23andMe", "TNT-23ANDME"),
        ("238 Sand Island Property", "TNT-238-SAND-ISLAND-PROPERTY"),
        ("Tavern & Bowl", "TNT-TAVERN-BOWL"),
    ] {
        assert_eq!(candidate_tnt_id(label).unwrap(), expected);
    }
}

#[test]
fn cmbs_tenant_id_allocator_replays_same_key_without_row_or_batch_inputs() {
    let request = CmbsTenantIdAllocationRequest::new(
        "24 Hour Fitness",
        "24 hour fitness",
        "blake3:cmbs-registry-20260625",
        "blake3:cmbs-alias-patch-002",
        "review:cmbs:24-hour-fitness:001",
    );
    let reserved = CmbsTenantReservedId::new("TNT-24-HOUR-FITNESS", request.replay_key());
    let allocator = CmbsTenantIdAllocator::new([reserved]);

    let replay = allocator
        .allocate(&request)
        .expect("identical replay key reuses existing ID");

    assert_eq!(replay.canonical_id, "TNT-24-HOUR-FITNESS");
    assert_eq!(replay.replay_key, request.replay_key());
}

#[test]
fn cmbs_tenant_id_allocator_refuses_collision_without_silent_suffixes() {
    let existing = sears_request("blake3:cmbs-registry-20260625");
    let mut requested = sears_request("blake3:cmbs-registry-20260625");
    requested.review_decision_id = "review:cmbs:sears:auto-center:001".to_string();
    let allocator = CmbsTenantIdAllocator::new([CmbsTenantReservedId::new(
        "TNT-SEARS",
        existing.replay_key(),
    )]);

    let refusal = allocator
        .allocate(&requested)
        .expect_err("different replay key must not get TNT-SEARS-2");

    assert_eq!(refusal.code, RefusalCode::EEntityPatchConflict);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|hint| hint.contains("Resolve the patch conflict"))
    );
    assert_eq!(candidate_tnt_id("Sears").unwrap(), "TNT-SEARS");
}

#[test]
fn cmbs_tenant_id_allocator_refuses_stale_registry_snapshot_before_promotion() {
    let existing = sears_request("blake3:cmbs-registry-20260625");
    let requested = sears_request("blake3:cmbs-registry-20260626");
    let allocator = CmbsTenantIdAllocator::new([CmbsTenantReservedId::new(
        "TNT-SEARS",
        existing.replay_key(),
    )]);

    let refusal = allocator
        .allocate(&requested)
        .expect_err("snapshot drift must refuse rather than remint");

    assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|hint| hint.contains("matching registry snapshot"))
    );
}

#[test]
fn profile_firewall_tenant_id_namespace_rejects_cross_profile_reuse() {
    let existing = CmbsTenantReservedId::new(
        "TNT-WELLS-FARGO-BANK",
        CmbsTenantIdAllocationRequest {
            profile_id: "regab_firm_identity".to_string(),
            profile_version: "0.1.0".to_string(),
            canonical_type: "organization".to_string(),
            identity_semantics: "same_firm_or_reviewed_alias".to_string(),
            reviewed_display_label: "Wells Fargo Bank".to_string(),
            normalized_display_label: "wells fargo bank".to_string(),
            registry_snapshot_hash: "blake3:regab-registry".to_string(),
            alias_patch_hash: "blake3:regab-alias-patch".to_string(),
            review_decision_id: "review:regab:wells-fargo:001".to_string(),
        }
        .replay_key(),
    );
    let allocator = CmbsTenantIdAllocator::new([existing]);
    let request = CmbsTenantIdAllocationRequest::new(
        "Wells Fargo Bank",
        "wells fargo bank",
        "blake3:cmbs-registry",
        "blake3:cmbs-alias-patch",
        "review:cmbs:wells-fargo-bank:001",
    );

    let refusal = allocator
        .allocate(&request)
        .expect_err("TNT ids cannot be same-as evidence for Reg AB profiles");

    assert_eq!(refusal.code, RefusalCode::EEntityProfile);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|hint| hint.contains("relation hint"))
    );
}

fn sears_request(registry_snapshot_hash: &str) -> CmbsTenantIdAllocationRequest {
    CmbsTenantIdAllocationRequest::new(
        "Sears",
        "sears",
        registry_snapshot_hash,
        "blake3:cmbs-alias-patch-001",
        "review:cmbs:sears:001",
    )
}
