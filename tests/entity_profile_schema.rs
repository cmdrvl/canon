use canon::{
    RefusalCode,
    entity::profile::{EntityProfileDocument, EntityProfileError},
};
use serde_json::json;

#[test]
fn entity_profile_schema_validates_cmbs_tenant_label_profile() {
    let profile = EntityProfileDocument::from_yaml_str(include_str!(
        "fixtures/entity/profiles/cmbs_tenant_label.yaml"
    ))
    .expect("cmbs tenant profile validates");

    assert_eq!(profile.profile, "cmbs_tenant_label");
    assert_eq!(profile.entity_type, "tenant_label");
    assert_eq!(profile.identity_semantics, "canonical_display_label");
    assert_eq!(profile.canonical_type, "tenant_label");
    assert!(
        profile
            .required_fields
            .contains(&"raw_tenant_name".to_string())
    );
    assert_eq!(
        profile.patch_namespaces.aliases,
        "cmbs_tenant_label.aliases"
    );

    let reference = profile.to_reference();
    assert!(reference.is_complete());
    profile
        .validate_matches(&reference)
        .expect("matching profile reference validates");
}

#[test]
fn entity_profile_schema_validates_regab_firm_identity_profile() {
    let profile = EntityProfileDocument::from_yaml_str(include_str!(
        "fixtures/entity/profiles/regab_firm_identity.yaml"
    ))
    .expect("regab firm profile validates");

    assert_eq!(profile.profile, "regab_firm_identity");
    assert_eq!(profile.entity_type, "organization");
    assert_eq!(profile.identity_semantics, "same_firm_or_reviewed_alias");
    assert_eq!(profile.canonical_type, "org");
    assert!(profile.required_fields.contains(&"org_name".to_string()));
    assert!(
        profile
            .evidence
            .cannot_link
            .iter()
            .any(|operator| operator.op == "division_boundary")
    );
}

#[test]
fn entity_profile_schema_missing_required_fields_refuses_with_entity_profile() {
    let error = EntityProfileDocument::from_yaml_str(
        r#"
profile: cmbs_tenant_label
version: 0.1.0
entity_type: tenant_label
identity_semantics: canonical_display_label
required_fields: []
"#,
    )
    .expect_err("missing canonical_type and schema sections refuses");

    assert_entity_error_code(&error, RefusalCode::EEntityProfile);
    assert!(
        error.detail["missing"]
            .as_array()
            .unwrap()
            .contains(&json!("canonical_type"))
    );
    assert!(
        error.detail["missing"]
            .as_array()
            .unwrap()
            .contains(&json!("normalized_views"))
    );
}

#[test]
fn entity_strategy_validation_unsupported_operator_refuses_with_entity_strategy() {
    let error = EntityProfileDocument::from_yaml_str(
        r#"
profile: cmbs_tenant_label
version: 0.1.0
entity_type: tenant_label
identity_semantics: canonical_display_label
canonical_type: tenant_label
required_fields: [source_row_id, raw_tenant_name]
normalized_views:
  tenant_core:
    operators: [unicode_fold, lowercase, normalize_whitespace]
evidence:
  support:
    - op: neural_embedding_similarity
      view: tenant_core
  cannot_link:
    - op: protected_token_conflict
      view: tenant_core
  relation_hints:
    - op: related_brand_family
patch_namespaces:
  aliases: cmbs_tenant_label.aliases
  distinct: cmbs_tenant_label.distinct
  relations: cmbs_tenant_label.relations
"#,
    )
    .expect_err("unsupported support operator refuses");

    assert_entity_error_code(&error, RefusalCode::EEntityStrategy);
    assert_eq!(error.detail["operator"], "neural_embedding_similarity");
    assert_eq!(error.detail["lane"], "support");
}

#[test]
fn entity_strategy_validation_detects_profile_mismatch_before_stage_run() {
    let profile = EntityProfileDocument::from_yaml_str(include_str!(
        "fixtures/entity/profiles/cmbs_tenant_label.yaml"
    ))
    .expect("cmbs tenant profile validates");
    let mut expected = profile.to_reference();
    expected.id = "regab_firm_identity".to_string();
    expected.entity_type = "organization".to_string();
    expected.identity_semantics = "same_firm_or_reviewed_alias".to_string();
    expected.canonical_type = "org".to_string();

    let error = profile
        .validate_matches(&expected)
        .expect_err("mismatched profile reference refuses");

    assert_entity_error_code(&error, RefusalCode::EEntityProfile);
    let mismatches = error.detail["mismatches"].as_array().unwrap();
    assert!(mismatches.iter().any(|entry| entry["field"] == "profile"));
    assert!(
        mismatches
            .iter()
            .any(|entry| entry["field"] == "identity_semantics")
    );
}

#[test]
fn entity_strategy_validation_keeps_cross_profile_alignment_as_relation_hint_only() {
    let profile = EntityProfileDocument::from_yaml_str(include_str!(
        "fixtures/entity/profiles/cmbs_tenant_label.yaml"
    ))
    .expect("cross-profile relation hint is allowed");
    assert!(
        profile
            .evidence
            .relation_hints
            .iter()
            .any(|operator| operator.op == "cross_profile_alignment")
    );

    let error = EntityProfileDocument::from_yaml_str(
        r#"
profile: cmbs_tenant_label
version: 0.1.0
entity_type: tenant_label
identity_semantics: canonical_display_label
canonical_type: tenant_label
required_fields: [source_row_id, raw_tenant_name]
normalized_views:
  tenant_core:
    operators: [unicode_fold, lowercase, normalize_whitespace]
evidence:
  support:
    - op: cross_profile_alignment
      view: tenant_core
  cannot_link:
    - op: protected_token_conflict
      view: tenant_core
  relation_hints:
    - op: related_brand_family
patch_namespaces:
  aliases: cmbs_tenant_label.aliases
  distinct: cmbs_tenant_label.distinct
  relations: cmbs_tenant_label.relations
"#,
    )
    .expect_err("cross-profile support evidence refuses");

    assert_entity_error_code(&error, RefusalCode::EEntityStrategy);
    assert_eq!(error.detail["operator"], "cross_profile_alignment");
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
