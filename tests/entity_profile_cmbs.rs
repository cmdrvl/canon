use canon::entity::profile::EntityProfileDocument;

const CMBS_PROFILE: &str = include_str!("fixtures/entity/profiles/cmbs_tenant_label.yaml");

#[test]
fn cmbs_tenant_profile_declares_display_label_identity_and_required_fields() {
    let profile =
        EntityProfileDocument::from_yaml_str(CMBS_PROFILE).expect("cmbs tenant profile validates");

    assert_eq!(profile.profile, "cmbs_tenant_label");
    assert_eq!(profile.entity_type, "tenant_label");
    assert_eq!(profile.identity_semantics, "canonical_display_label");
    assert_eq!(profile.canonical_type, "tenant_label");
    assert_eq!(
        profile.required_fields,
        [
            "source_row_id",
            "deal_id",
            "loan_id",
            "property_id",
            "raw_tenant_name"
        ]
    );
}

#[test]
fn cmbs_tenant_profile_keeps_relation_hints_out_of_same_as_support() {
    let profile =
        EntityProfileDocument::from_yaml_str(CMBS_PROFILE).expect("cmbs tenant profile validates");

    assert!(profile.normalized_views.contains_key("tenant_core"));
    assert!(profile.normalized_views.contains_key("tenant_tokens"));
    assert!(profile.normalized_views.contains_key("tenant_brand"));
    assert!(
        profile
            .evidence
            .support
            .iter()
            .all(|operator| operator.op != "cross_profile_alignment")
    );
    assert!(
        profile
            .evidence
            .cannot_link
            .iter()
            .any(|operator| operator.op == "same_property_distinct_rank")
    );

    for operator in &profile.evidence.relation_hints {
        assert_eq!(
            operator.params.get("merge_authorized").map(String::as_str),
            Some("false")
        );
        assert_eq!(
            operator.params.get("review_policy").map(String::as_str),
            Some("relation_hint_only")
        );
    }
}
