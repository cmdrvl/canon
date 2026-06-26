#![forbid(unsafe_code)]

use canon::{
    entity::profiles::cmbs::{
        CmbsTenantIdAllocationRequest, CmbsTenantIdAllocator, candidate_tnt_id,
    },
    namekit::tenant::{cmbs_tenant_pair_evidence, normalize_cmbs_tenant},
};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

const INTEGRATION_CASES: &str =
    include_str!("../fixtures/entity/profiles/cmbs_tenant_label/integration_cases.json");

#[test]
#[allow(non_snake_case)]
fn CMBS_I001_profile_fixture_promotes_tnt_sears_with_exact_alias_material() {
    let fixture = integration_cases();
    let case = case_by_id(&fixture, "CMBS-I001");

    assert_profile_header(&fixture);
    assert_eq!(case["expected_canonical_id"], "TNT-SEARS");
    assert_eq!(
        candidate_tnt_id(case["reviewed_display_label"].as_str().unwrap()).unwrap(),
        "TNT-SEARS"
    );

    let request = CmbsTenantIdAllocationRequest::new(
        case["reviewed_display_label"].as_str().unwrap(),
        case["normalized_display_label"].as_str().unwrap(),
        case["registry_snapshot_hash"].as_str().unwrap(),
        case["alias_patch_hash"].as_str().unwrap(),
        case["review_decision_id"].as_str().unwrap(),
    );
    let allocation = CmbsTenantIdAllocator::default()
        .allocate(&request)
        .expect("TNT-SEARS allocation succeeds");
    assert_eq!(allocation.canonical_id, "TNT-SEARS");
    assert_eq!(allocation.side_effects.registry_writes, 0);

    let aliases = case["exact_alias_material"].as_array().unwrap();
    assert_eq!(aliases.len(), 4);
    let alias_inputs = aliases
        .iter()
        .map(|alias| alias["input"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        alias_inputs,
        ["Sears", "SEARS LLC", "Sears Roebuck & Co.", "Sears #1234"]
    );
    for alias in aliases {
        assert_eq!(alias["rule_id"], "CMBS_TENANT_REVIEWED_ALIAS");
        assert_eq!(alias["source"], "review_decision");
        let evidence = cmbs_tenant_pair_evidence("Sears", alias["input"].as_str().unwrap());
        assert!(
            evidence.same_tenant_label_support,
            "Sears should support alias {}",
            alias["input"]
        );
    }
}

#[test]
#[allow(non_snake_case)]
fn CMBS_I002_profile_fixture_keeps_sears_family_and_protected_tokens_distinct() {
    let fixture = integration_cases();
    let case = case_by_id(&fixture, "CMBS-I002");
    assert_eq!(case["expected_auto_merge"], false);
    assert_eq!(case["expected_outcome"], "relation_or_distinct_review_only");

    let protected_terms = string_set(&case["protected_distinction_terms"]);
    for required in [
        "auto",
        "center",
        "kmart",
        "transform",
        "sr",
        "holdings",
        "management",
        "capital",
    ] {
        assert!(protected_terms.contains(required));
    }

    for pair in case["hard_negative_pairs"].as_array().unwrap() {
        let pair = pair.as_array().unwrap();
        let left = pair[0].as_str().unwrap();
        let right = pair[1].as_str().unwrap();
        let evidence = cmbs_tenant_pair_evidence(left, right);
        assert!(
            !evidence.same_tenant_label_support,
            "{left} and {right} must not be same tenant-label support"
        );
        let left_norm = normalize_cmbs_tenant(left);
        let right_norm = normalize_cmbs_tenant(right);
        assert!(
            !left_norm.protected_tokens.is_empty()
                || !right_norm.protected_tokens.is_empty()
                || evidence.requires_review,
            "{left} vs {right} must retain a protected/review signal"
        );
    }
}

#[test]
#[allow(non_snake_case)]
fn CMBS_I003_profile_fixture_records_500k_row_gates_without_static_bulk_data() {
    let fixture = integration_cases();
    let case = case_by_id(&fixture, "CMBS-I003");
    assert_eq!(case["generator"], "deterministic_seeded_operator_tier");
    assert_eq!(case["generator_bead"], "bd-1pz.7");
    assert_eq!(case["seed"], 424242);
    assert_eq!(case["row_count"], 500_000);
    assert_eq!(case["tenant_observation_count"], 500_000);
    assert_eq!(case["expected_profile"], "cmbs_tenant_label");

    let gates = string_set(&case["gates"]);
    for gate in ["G05", "G08", "G11"] {
        assert!(gates.contains(gate), "CMBS-I003 must record {gate}");
    }

    let assertions = &case["assertions"];
    assert_eq!(assertions["exact_bucket_pair_expansion_count"], 0);
    assert!(
        assertions["candidate_pairs_per_surface_p95_max"]
            .as_u64()
            .unwrap()
            <= 25
    );
    assert!(
        assertions["candidate_pairs_per_surface_p99_max"]
            .as_u64()
            .unwrap()
            <= 100
    );
    assert!(assertions["review_item_count_max"].as_u64().unwrap() <= 2_000);
    assert_eq!(assertions["telemetry_required"], true);
    assert_eq!(assertions["giant_static_fixture_committed"], false);
}

#[test]
fn cmbs_profile_fixture_semantics_are_display_label_not_legal_entity_identity() {
    let fixture = integration_cases();
    assert_profile_header(&fixture);
    assert_eq!(
        fixture["profile_firewall"]["tenant_label_ids_are_not_legal_entity_ids"],
        true
    );
    assert_eq!(
        fixture["profile_firewall"]["canonical_display_label_only"],
        true
    );
    assert_eq!(
        fixture["profile_firewall"]["cross_profile_same_as_allowed"],
        false
    );
    assert_eq!(
        fixture["profile_firewall"]["relation_hints_belong_to_ontology_layer"],
        true
    );

    let non_goals = string_set(&fixture["non_goals"]);
    for required in [
        "do not claim legal same-firm identity",
        "do not commit generated 500k-row artifacts",
    ] {
        assert!(non_goals.contains(required));
    }
    assert!(
        non_goals
            .iter()
            .any(|goal| goal.contains("network") && goal.contains("frontier model"))
    );

    for source in fixture["fixture_sources"].as_object().unwrap().values() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(source.as_str().unwrap());
        assert!(path.exists(), "fixture source missing: {}", path.display());
    }
}

fn integration_cases() -> Value {
    serde_json::from_str(INTEGRATION_CASES).expect("CMBS integration cases parse")
}

fn case_by_id<'a>(fixture: &'a Value, id: &str) -> &'a Value {
    fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap_or_else(|| panic!("missing {id}"))
}

fn assert_profile_header(fixture: &Value) {
    assert_eq!(
        fixture["schema_version"],
        "canon.entity.cmbs.profile_integration.v0"
    );
    assert_eq!(fixture["profile_id"], "cmbs_tenant_label");
    assert_eq!(fixture["entity_type"], "tenant_label");
    assert_eq!(fixture["identity_semantics"], "canonical_display_label");
    assert_eq!(fixture["canonical_type"], "tenant_label");
}

fn string_set(value: &Value) -> BTreeSet<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect()
}
