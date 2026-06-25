use canon::entity::prepare::{
    PrepareInputContract, project_prepare_csv_reader, project_prepare_jsonl_reader,
};
use canon::entity::profile::EntityProfileDocument;
use serde_json::{Value, json};
use std::io::Cursor;

const CMBS_PROFILE: &str = include_str!("fixtures/entity/profiles/cmbs_tenant_label.yaml");
const REGAB_PROFILE: &str = include_str!("fixtures/entity/profiles/regab_firm_identity.yaml");
const BAD_ALIAS: &str = include_str!("fixtures/entity/prepare/input_contract/bad_alias.jsonl");

#[test]
fn entity_prepare_input_contract() {
    let profile = EntityProfileDocument::from_yaml_str(CMBS_PROFILE).expect("valid profile");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");
    let input = concat!(
        "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
        "row-1,deal-1,loan-7,prop-2, SEARS LLC ,\"[\"\"Sears\"\",\"\"Sears Roebuck\"\"]\",\"[\"\"Store 101\"\"]\"\n",
        "row-2,deal-1,loan-8,prop-2,Sears LLC,,[]\n"
    );

    let observations = project_prepare_csv_reader(Cursor::new(input), b',', &contract)
        .expect("prepare CSV projects");

    assert_eq!(observations.len(), 2);
    let first = &observations[0];
    assert_eq!(first.profile_id, "cmbs_tenant_label");
    assert_eq!(first.primary_surface.value, "SEARS LLC");
    assert_eq!(first.primary_surface.field, "raw_tenant_name");
    assert_eq!(
        first
            .alias_surfaces
            .iter()
            .map(|surface| surface.value.as_str())
            .collect::<Vec<_>>(),
        ["Sears", "Sears Roebuck"]
    );
    assert_eq!(first.mention_surfaces[0].value, "Store 101");
    assert_eq!(first.context["deal_id"], json!("deal-1"));
    assert_eq!(first.context["loan_id"], json!("loan-7"));
    assert!(!first.context.contains_key("source_row_id"));
    assert_eq!(first.provenance["source_row_id"], "row-1");
    assert_eq!(first.provenance["deal_id"], "deal-1");
}

#[test]
#[allow(non_snake_case)]
fn EN_P003_malformed_alias_surfaces_json_refuses_with_row_field_and_sample() {
    let profile = EntityProfileDocument::from_yaml_str(CMBS_PROFILE).expect("valid profile");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");

    let refusal = project_prepare_jsonl_reader(Cursor::new(BAD_ALIAS), &contract)
        .expect_err("bad alias JSON refuses");
    let payload = serde_json::to_value(&refusal).expect("refusal serializes");

    assert_eq!(payload["code"], "E_ENTITY_INPUT_CONTRACT");
    assert!(
        payload["message"]
            .as_str()
            .unwrap()
            .contains("alias_surfaces_json")
    );
    assert_eq!(payload["detail"]["row_number"], 1);
    assert_eq!(payload["detail"]["field"], "alias_surfaces_json");
    assert_eq!(payload["detail"]["sample"], "[\"Sears\",");
    assert!(payload["detail"].get("error").is_some());
}

#[test]
fn regab_prepare_jsonl_maps_mentions_anchors_and_context_without_source_row_identity() {
    let profile = EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("valid profile");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");
    let input = json!({
        "source_row_id": "sec10d-1",
        "field_name": "servicer",
        "org_name": "PNC Bank N.A.",
        "dataset": "sec10d",
        "alias_surfaces_json": ["PNC Bank, National Association"],
        "mention_surfaces_json": "[\"PNC\"]",
        "filing_cik": "0001234567",
        "accession": "0001234567-26-000001",
        "role_context": "servicer",
        "capacity": "master servicer",
        "subject_role": "reporting party"
    });
    let input = format!("{input}\n");

    let observations = project_prepare_jsonl_reader(Cursor::new(input), &contract)
        .expect("prepare JSONL projects");
    let observation = &observations[0];

    assert_eq!(observation.primary_surface.value, "PNC Bank N.A.");
    assert_eq!(
        observation.alias_surfaces[0].value,
        "PNC Bank, National Association"
    );
    assert_eq!(observation.mention_surfaces[0].value, "PNC");
    assert_eq!(
        observation
            .anchors
            .iter()
            .map(|anchor| (anchor.namespace.as_str(), anchor.value.as_str()))
            .collect::<Vec<_>>(),
        [("accession", "0001234567-26-000001"), ("cik", "0001234567")]
    );
    assert_eq!(observation.context["dataset"], json!("sec10d"));
    assert_eq!(observation.context["field_name"], json!("servicer"));
    assert!(!observation.context.contains_key("source_row_id"));
    assert_eq!(observation.provenance["source_row_id"], "sec10d-1");
}

#[test]
fn prepare_contract_missing_required_profile_field_names_available_headers() {
    let profile = EntityProfileDocument::from_yaml_str(CMBS_PROFILE).expect("valid profile");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");
    let input = "source_row_id,raw_tenant_name\nrow-1,Sears\n";

    let refusal = project_prepare_csv_reader(Cursor::new(input), b',', &contract)
        .expect_err("missing deal_id refuses");
    let payload: Value = serde_json::to_value(&refusal).expect("refusal serializes");

    assert_eq!(payload["code"], "E_ENTITY_INPUT_CONTRACT");
    assert_eq!(payload["detail"]["field"], "deal_id");
    assert_eq!(
        payload["detail"]["available_fields"],
        json!(["source_row_id", "raw_tenant_name"])
    );
}
