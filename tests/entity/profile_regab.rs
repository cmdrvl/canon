use canon::entity::profile::EntityProfileDocument;

const REGAB_PROFILE: &str = include_str!("../fixtures/entity/profiles/regab_firm_identity.yaml");

#[test]
fn regab_firm_profile_declares_reviewed_firm_identity() {
    let profile =
        EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("regab profile validates");

    assert_eq!(profile.profile, "regab_firm_identity");
    assert_eq!(profile.entity_type, "organization");
    assert_eq!(profile.identity_semantics, "same_firm_or_reviewed_alias");
    assert_eq!(profile.canonical_type, "org");
    assert_eq!(
        profile.required_fields,
        ["source_row_id", "field_name", "org_name", "dataset"]
    );
}

#[test]
fn regab_firm_profile_keeps_tenant_label_semantics_out() {
    let profile =
        EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("regab profile validates");

    assert!(
        profile
            .evidence
            .support
            .iter()
            .any(|operator| operator.op == "reviewed_alias")
    );
    assert!(
        profile
            .evidence
            .cannot_link
            .iter()
            .any(|operator| operator.op == "role_conflict")
    );
    assert!(
        profile
            .evidence
            .relation_hints
            .iter()
            .any(|operator| operator.op == "parent_subsidiary_context")
    );
    assert!(
        profile
            .evidence
            .support
            .iter()
            .all(|operator| operator.op != "related_brand_family")
    );
}
