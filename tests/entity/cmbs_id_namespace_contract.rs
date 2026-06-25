use canon::{
    RefusalCode,
    entity::profile::{EntityProfileDocument, EntityProfileError, EntityProfileFirewall},
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct NamespaceContract {
    schema_version: String,
    profile: ProfileContract,
    id_namespace: IdNamespaceContract,
    allocator: AllocatorContract,
    exact_replay: ExactReplayContract,
    collision_refusals: Vec<CollisionRefusalContract>,
    cross_profile_firewall: CrossProfileFirewallContract,
}

#[derive(Debug, Deserialize)]
struct ProfileContract {
    id: String,
    version: String,
    entity_type: String,
    identity_semantics: String,
    canonical_type: String,
    patch_namespaces: PatchNamespacesContract,
}

#[derive(Debug, Deserialize)]
struct PatchNamespacesContract {
    aliases: String,
    distinct: String,
    relations: String,
}

#[derive(Debug, Deserialize)]
struct IdNamespaceContract {
    prefix: String,
    canonical_id_pattern: String,
    valid_examples: Vec<String>,
    invalid_examples: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AllocatorContract {
    stage: String,
    inputs: Vec<AllocatorInputContract>,
    forbidden_identity_inputs: Vec<String>,
    default_candidate_id: CandidateIdContract,
}

#[derive(Debug, Deserialize)]
struct AllocatorInputContract {
    name: String,
    required: bool,
    participates_in_replay_key: bool,
}

#[derive(Debug, Deserialize)]
struct CandidateIdContract {
    source: String,
    normalization: String,
    collision_policy: String,
}

#[derive(Debug, Deserialize)]
struct ExactReplayContract {
    stable_key_fields: Vec<String>,
    disallowed_key_fields: Vec<String>,
    expectations: Vec<ReplayExpectationContract>,
}

#[derive(Debug, Deserialize)]
struct ReplayExpectationContract {
    id: String,
    left: ReplayInputContract,
    right: ReplayInputContract,
    expected_canonical_id: String,
    expected_relation: String,
}

#[derive(Debug, Deserialize)]
struct ReplayInputContract {
    profile_id: String,
    canonical_type: String,
    normalized_display_label: String,
    registry_snapshot_hash: String,
    alias_patch_hash: String,
    review_decision_id: String,
    row_order: u64,
    physical_batch_id: String,
}

#[derive(Debug, Deserialize)]
struct CollisionRefusalContract {
    id: String,
    proposed_canonical_id: String,
    refusal_code: String,
    behavior: String,
    next_command_contains: String,
}

#[derive(Debug, Deserialize)]
struct CrossProfileFirewallContract {
    relation_policy: String,
    same_as_allowed: bool,
    refusal_code: String,
    examples: Vec<CrossProfileExampleContract>,
}

#[derive(Debug, Deserialize)]
struct CrossProfileExampleContract {
    source_profile: String,
    source_canonical_id: String,
    target_profile: String,
    target_canonical_id: String,
}

#[test]
fn cmbs_id_namespace_contract_defines_tnt_shape_and_profile_metadata() {
    let contract = namespace_contract();
    let profile = cmbs_profile();

    assert_eq!(
        contract.schema_version,
        "canon.entity.cmbs_id_namespace_contract.v0"
    );
    assert_eq!(contract.profile.id, profile.profile);
    assert_eq!(contract.profile.version, profile.version);
    assert_eq!(contract.profile.entity_type, profile.entity_type);
    assert_eq!(
        contract.profile.identity_semantics,
        profile.identity_semantics
    );
    assert_eq!(contract.profile.canonical_type, profile.canonical_type);
    assert_eq!(
        contract.profile.patch_namespaces.aliases,
        profile.patch_namespaces.aliases
    );
    assert_eq!(
        contract.profile.patch_namespaces.distinct,
        profile.patch_namespaces.distinct
    );
    assert_eq!(
        contract.profile.patch_namespaces.relations,
        profile.patch_namespaces.relations
    );

    assert_eq!(contract.id_namespace.prefix, "TNT");
    assert_eq!(
        contract.id_namespace.canonical_id_pattern,
        "TNT-[A-Z0-9]+(?:-[A-Z0-9]+)*"
    );
    for example in &contract.id_namespace.valid_examples {
        assert!(is_valid_tnt_id(example), "{example} should match TNT shape");
    }
    for example in &contract.id_namespace.invalid_examples {
        assert!(
            !is_valid_tnt_id(example),
            "{example} should be rejected by TNT shape"
        );
    }
}

#[test]
fn cmbs_id_namespace_contract_allocator_inputs_are_exact_replay_key() {
    let contract = namespace_contract();

    assert_eq!(contract.allocator.stage, "promote");
    assert_eq!(
        contract.allocator.default_candidate_id.source,
        "reviewed_display_label"
    );
    assert_eq!(
        contract.allocator.default_candidate_id.normalization,
        "uppercase_ascii_slug"
    );
    assert_eq!(
        contract.allocator.default_candidate_id.collision_policy,
        "refuse_without_suffix_or_silent_remint"
    );

    for field in &[
        "profile_id",
        "canonical_type",
        "identity_semantics",
        "normalized_display_label",
        "registry_snapshot_hash",
        "alias_patch_hash",
        "review_decision_id",
    ] {
        let input = allocator_input(&contract, field);
        assert!(input.required, "{field} must be required");
        assert!(
            input.participates_in_replay_key,
            "{field} must participate in exact replay"
        );
    }

    for field in &[
        "source_row_id",
        "deal_id",
        "loan_id",
        "property_id",
        "row_order",
        "physical_batch_id",
    ] {
        assert!(
            contract
                .allocator
                .forbidden_identity_inputs
                .iter()
                .any(|input| input == field),
            "{field} must not allocate tenant identity"
        );
        assert!(
            contract
                .exact_replay
                .disallowed_key_fields
                .iter()
                .any(|input| input == field),
            "{field} must not affect replay keys"
        );
    }
}

#[test]
fn cmbs_id_namespace_contract_exact_replay_ignores_batch_and_row_order() {
    let contract = namespace_contract();

    assert_eq!(
        contract.exact_replay.stable_key_fields,
        [
            "profile_id",
            "canonical_type",
            "identity_semantics",
            "normalized_display_label",
            "registry_snapshot_hash",
            "alias_patch_hash",
            "review_decision_id",
        ]
    );
    for expectation in &contract.exact_replay.expectations {
        assert_eq!(
            expectation.expected_relation, "same_id",
            "{}",
            expectation.id
        );
        assert!(
            is_valid_tnt_id(&expectation.expected_canonical_id),
            "{} expected invalid TNT id",
            expectation.id
        );
        assert_same_replay_key(expectation);
        assert_ne!(
            expectation.left.row_order, expectation.right.row_order,
            "{} must prove row order is not identity",
            expectation.id
        );
        assert_ne!(
            expectation.left.physical_batch_id, expectation.right.physical_batch_id,
            "{} must prove physical batch is not identity",
            expectation.id
        );
    }
}

#[test]
fn cmbs_id_namespace_contract_refuses_collisions_without_silent_suffixes() {
    let contract = namespace_contract();

    assert_eq!(contract.collision_refusals.len(), 2);
    for refusal in &contract.collision_refusals {
        assert!(
            is_valid_tnt_id(&refusal.proposed_canonical_id),
            "{} collision case proposes invalid TNT id",
            refusal.id
        );
        assert!(
            ["E_ENTITY_PATCH_CONFLICT", "E_ENTITY_REGISTRY_SNAPSHOT"]
                .contains(&refusal.refusal_code.as_str()),
            "{} has unexpected refusal code {}",
            refusal.id,
            refusal.refusal_code
        );
        assert_eq!(refusal.behavior, "refuse_no_mutation");
        assert!(!refusal.next_command_contains.trim().is_empty());
    }
}

#[test]
fn tenant_profile_firewall_contract_disallows_cross_profile_same_as_for_tnt_ids() {
    let contract = namespace_contract();
    let firewall = &contract.cross_profile_firewall;

    assert_eq!(firewall.relation_policy, "relation_hint_only");
    assert!(!firewall.same_as_allowed);
    assert_eq!(firewall.refusal_code, "E_ENTITY_PROFILE");
    assert!(!firewall.examples.is_empty());

    let cmbs = cmbs_profile();
    let regab = regab_profile();
    let source = EntityProfileFirewall::new(
        &cmbs,
        "blake3:cmbs-strategy",
        "blake3:cmbs-registry",
        cmbs.patch_namespaces.aliases.clone(),
    )
    .expect("CMBS profile firewall validates");
    let target = EntityProfileFirewall::new(
        &regab,
        "blake3:regab-strategy",
        "blake3:regab-registry",
        regab.patch_namespaces.aliases.clone(),
    )
    .expect("Reg AB profile firewall validates");
    let error = source
        .validate_same_as_reuse(&target)
        .expect_err("tenant label ID must not become firm same-as evidence");

    assert_entity_error_code(&error, RefusalCode::EEntityProfile);
    for example in &firewall.examples {
        assert_eq!(example.source_profile, "cmbs_tenant_label");
        assert!(
            example.source_canonical_id.starts_with("TNT-"),
            "{} must stay in tenant namespace",
            example.source_canonical_id
        );
        assert_eq!(example.target_profile, "regab_firm_identity");
        assert!(
            !example.target_canonical_id.starts_with("TNT-"),
            "{} must stay out of tenant namespace",
            example.target_canonical_id
        );
    }
}

fn namespace_contract() -> NamespaceContract {
    serde_json::from_str(include_str!(
        "../fixtures/entity/cmbs/id_namespace/namespace_contract.json"
    ))
    .expect("CMBS namespace contract fixture parses")
}

fn cmbs_profile() -> EntityProfileDocument {
    EntityProfileDocument::from_yaml_str(include_str!(
        "../fixtures/entity/profiles/cmbs_tenant_label.yaml"
    ))
    .expect("CMBS tenant profile validates")
}

fn regab_profile() -> EntityProfileDocument {
    EntityProfileDocument::from_yaml_str(include_str!(
        "../fixtures/entity/profiles/regab_firm_identity.yaml"
    ))
    .expect("Reg AB firm profile validates")
}

fn allocator_input<'a>(contract: &'a NamespaceContract, field: &str) -> &'a AllocatorInputContract {
    contract
        .allocator
        .inputs
        .iter()
        .find(|input| input.name == field)
        .unwrap_or_else(|| panic!("allocator input {field} exists"))
}

fn assert_same_replay_key(expectation: &ReplayExpectationContract) {
    assert_eq!(expectation.left.profile_id, expectation.right.profile_id);
    assert_eq!(
        expectation.left.canonical_type,
        expectation.right.canonical_type
    );
    assert_eq!(
        expectation.left.normalized_display_label,
        expectation.right.normalized_display_label
    );
    assert_eq!(
        expectation.left.registry_snapshot_hash,
        expectation.right.registry_snapshot_hash
    );
    assert_eq!(
        expectation.left.alias_patch_hash,
        expectation.right.alias_patch_hash
    );
    assert_eq!(
        expectation.left.review_decision_id,
        expectation.right.review_decision_id
    );
}

fn is_valid_tnt_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("TNT-") else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    rest.split('-').all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    })
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
