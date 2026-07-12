use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/canon_v1/quality/ablation_cases.json"
    ))
    .expect("ablation fixture parses")
}

fn string_set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn array<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} must be an array"))
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be a string"))
}

#[test]
fn ablation_fixture_declares_required_families_without_default_thresholds() {
    let fixture = fixture();
    assert_eq!(
        fixture["schema_version"],
        "canon.entity.evidence_ablation_cases.v0"
    );
    assert_eq!(
        fixture["policy"]["domain_specific_shortcuts_allowed"],
        false
    );
    assert!(
        fixture["policy"]["default_thresholds"]
            .as_array()
            .expect("default_thresholds array")
            .is_empty(),
        "ablation fixture must not define Canon default thresholds"
    );

    let required_families = string_set(&[
        "exact_alias",
        "normalized_name",
        "char_token_similarity",
        "sparse_retrieval",
        "trusted_anchors",
        "address_web_domain_anchors",
        "contextual_cooccurrence",
        "source_priors",
        "relationship_evidence",
        "full_system_union",
    ]);
    let families = array(&fixture, "evidence_families");
    let mut seen = BTreeSet::new();
    for family in families {
        let family_id = text(family, "family_id");
        assert!(seen.insert(family_id.to_string()), "duplicate {family_id}");
        assert_eq!(
            family["default_threshold"],
            Value::Null,
            "{family_id} must not carry a default threshold"
        );
    }
    assert_eq!(seen, required_families);

    let relationship = families
        .iter()
        .find(|family| family["family_id"] == "relationship_evidence")
        .expect("relationship family exists");
    assert_eq!(relationship["equivalence_default"], false);
    assert_eq!(
        relationship["requires_independent_equivalence_support"],
        true
    );
    assert_eq!(
        fixture["policy"]["relationship_evidence"]["equivalence_default"],
        false
    );
    assert_eq!(
        fixture["policy"]["relationship_evidence"]["requires_independent_equivalence_support"],
        true
    );
}

#[test]
fn ablation_fixture_declares_stage_local_miss_and_admission_codes() {
    let fixture = fixture();
    let required_stages = string_set(&[
        "normalization",
        "retrieval",
        "evidence_extraction",
        "scoring",
        "constraint",
        "cluster",
        "link",
        "policy",
    ]);
    let required_kinds = string_set(&["miss", "admission"]);

    let mut by_stage = BTreeMap::<String, BTreeSet<String>>::new();
    let mut codes = BTreeSet::<String>::new();
    for reason in array(&fixture, "stage_reason_codes") {
        let code = text(reason, "code");
        assert!(
            codes.insert(code.to_string()),
            "duplicate reason code {code}"
        );
        let stage = text(reason, "stage");
        let kind = text(reason, "kind");
        assert!(
            code == format!("{stage}.{kind}"),
            "reason code {code} must be stage-local"
        );
        by_stage
            .entry(stage.to_string())
            .or_default()
            .insert(kind.to_string());
    }

    assert_eq!(
        by_stage.keys().cloned().collect::<BTreeSet<_>>(),
        required_stages
    );
    for (stage, kinds) in by_stage {
        assert_eq!(kinds, required_kinds, "{stage} must have both reason kinds");
    }
}

#[test]
fn ablation_cases_cover_roles_for_each_family_with_hidden_labels_separated() {
    let fixture = fixture();
    let role_ids = array(&fixture, "roles")
        .iter()
        .map(|role| text(role, "role").to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        role_ids,
        string_set(&[
            "necessary",
            "misleading",
            "absent",
            "duplicated",
            "contradictory"
        ])
    );

    let reason_codes = array(&fixture, "stage_reason_codes")
        .iter()
        .map(|reason| text(reason, "code").to_string())
        .collect::<BTreeSet<_>>();
    let mut coverage = BTreeMap::<String, BTreeSet<String>>::new();

    for case in array(&fixture, "cases") {
        let family = text(case, "family_under_test");
        assert_hidden_labels_are_not_runtime_inputs(&case["runtime_inputs"], family);

        let hidden = &case["hidden_labels"];
        assert!(
            hidden.get("planted_pair_roles").is_some(),
            "{family} must have hidden planted labels"
        );

        for pair_role in array(hidden, "planted_pair_roles") {
            let role = text(pair_role, "role");
            assert!(role_ids.contains(role), "{family} uses unknown role {role}");
            coverage
                .entry(family.to_string())
                .or_default()
                .insert(role.to_string());

            let true_pair_id = text(pair_role, "true_pair_id");
            let false_pair_id = text(pair_role, "false_pair_id");
            assert_ne!(
                true_pair_id, false_pair_id,
                "{family}/{role} true and false planted pairs must differ"
            );
            assert!(
                reason_codes.contains(text(pair_role, "miss_reason")),
                "{family}/{role} miss reason must be declared"
            );
            assert!(
                reason_codes.contains(text(pair_role, "admission_reason")),
                "{family}/{role} admission reason must be declared"
            );
        }
    }

    let family_ids = array(&fixture, "evidence_families")
        .iter()
        .map(|family| text(family, "family_id").to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        coverage.keys().cloned().collect::<BTreeSet<_>>(),
        family_ids
    );
    for (family, roles) in coverage {
        assert_eq!(roles, role_ids, "{family} does not cover every role");
    }
}

#[test]
fn relationship_evidence_cases_are_non_equivalence_without_independent_support() {
    let fixture = fixture();
    let relationship = array(&fixture, "cases")
        .iter()
        .find(|case| case["family_under_test"] == "relationship_evidence")
        .expect("relationship evidence case exists");

    assert_eq!(
        relationship["hidden_labels"]["relationship_policy"],
        "non_equivalence_unless_separately_supported"
    );
    assert!(
        relationship["hidden_labels"]["expected_by_ablation"]["with_family_only"]
            .as_str()
            .expect("relationship expected text")
            .contains("no relationship-only pair may auto-link"),
        "relationship-only evidence must not imply equivalence"
    );
}

fn assert_hidden_labels_are_not_runtime_inputs(value: &Value, family: &str) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                assert!(
                    !matches!(
                        key.as_str(),
                        "hidden_labels" | "expected_by_ablation" | "oracle_pairs"
                    ),
                    "{family} leaked hidden label key {key} into runtime inputs"
                );
                assert_hidden_labels_are_not_runtime_inputs(child, family);
            }
        }
        Value::Array(items) => {
            for item in items {
                assert_hidden_labels_are_not_runtime_inputs(item, family);
            }
        }
        _ => {}
    }
}
