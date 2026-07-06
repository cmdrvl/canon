use canon::entity::{
    prepare::{PrepareInputContract, project_prepare_csv_reader},
    profile::EntityProfileDocument,
    runtime::{
        strategy::parse_strategy_bytes,
        types::{EntitySideField, EntityStrategy},
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

const REGAB_PROFILE: &str = include_str!("fixtures/entity/profiles/regab_firm_identity.yaml");
const REGAB_STRATEGY: &[u8] = include_bytes!("fixtures/entity/strategies/regab_firm_identity.yaml");

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/regab")
}

fn public_slice_root() -> PathBuf {
    fixture_root().join("sec10d_baseline_public")
}

fn read_json(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("json fixture opens"))
        .expect("json fixture parses")
}

#[test]
fn regab_firm_profile_validates_sec10d_profile_and_strategy_fixture() {
    let profile =
        EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("regab firm profile validates");
    assert_eq!(profile.profile, "regab_firm_identity");
    assert_eq!(profile.entity_type, "organization");
    assert_eq!(profile.identity_semantics, "same_firm_or_reviewed_alias");
    assert_eq!(profile.canonical_type, "org");
    assert_eq!(
        profile.required_fields,
        ["source_row_id", "field_name", "org_name", "dataset"]
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
            .cannot_link
            .iter()
            .any(|operator| operator.op == "division_boundary")
    );
    assert!(
        profile
            .evidence
            .relation_hints
            .iter()
            .any(|operator| operator.op == "parent_subsidiary_context")
    );

    let strategy = parse_strategy_bytes(REGAB_STRATEGY).expect("regab strategy validates");
    assert_regab_strategy_contract(&strategy);
}

#[test]
#[allow(non_snake_case)]
fn REGAB_I001_regab_firm_profile_accepts_org_mentions_shape() {
    let profile =
        EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("regab firm profile validates");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");
    let input =
        fs::read_to_string(public_slice_root().join("org_mentions_selected.csv")).expect("csv");

    let observations = project_prepare_csv_reader(Cursor::new(input), b',', &contract)
        .expect("regab org_mentions shape projects");

    assert_eq!(observations.len(), 46);
    let first = &observations[0];
    assert_eq!(first.profile_id, "regab_firm_identity");
    assert_eq!(first.primary_surface.field, "org_name");
    assert_eq!(first.primary_surface.value, "3650 REIT Loan Servicing LLC");
    assert_eq!(first.context["dataset"], json!("regab_servicer_schedules"));
    assert_eq!(first.context["field_name"], json!("servicer_name"));
    assert!(!first.context.contains_key("source_row_id"));
    assert_eq!(
        first
            .anchors
            .iter()
            .map(|anchor| (anchor.namespace.as_str(), anchor.field.as_str()))
            .collect::<Vec<_>>(),
        [("accession", "accession"), ("cik", "filing_cik")]
    );
    assert_eq!(
        first.mention_surfaces[0].value,
        "dataset_field=servicer_name"
    );
    assert_eq!(
        first.provenance["source_row_id"],
        "sec10d:blake3:ad482658f4e8061f1be96444cc345394f414dbc7325f822f7056442de5b1148e#servicer_name"
    );
}

#[test]
fn regab_firm_profile_missing_required_field_refuses_cleanly() {
    let profile =
        EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("regab firm profile validates");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");
    let input = "source_row_id,field_name,org_name\nrow-1,servicer_name,PNC Bank N.A.\n";

    let refusal = project_prepare_csv_reader(Cursor::new(input), b',', &contract)
        .expect_err("missing dataset refuses");
    let payload = serde_json::to_value(refusal).expect("refusal serializes");

    assert_eq!(payload["code"], "E_ENTITY_INPUT_CONTRACT");
    assert_eq!(payload["detail"]["field"], "dataset");
    assert_eq!(
        payload["detail"]["available_fields"],
        json!(["source_row_id", "field_name", "org_name"])
    );
}

#[test]
fn regab_firm_profile_excludes_certifying_party_person_fields_until_people_profile_exists() {
    let manifest = read_json(fixture_root().join("sec10d_regab_benchmark_manifest.json"));
    let columns = manifest["input_contract"]["org_mentions_columns"]
        .as_array()
        .expect("columns array")
        .iter()
        .map(|value| value.as_str().expect("column string"))
        .collect::<BTreeSet<_>>();
    assert!(!columns.contains("certifying_party_name"));

    let mut reader = csv::Reader::from_path(public_slice_root().join("org_mentions_selected.csv"))
        .expect("org_mentions csv opens");
    let field_names = reader
        .deserialize::<std::collections::BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv rows parse")
        .into_iter()
        .map(|row| row["field_name"].to_string())
        .collect::<BTreeSet<_>>();
    assert!(!field_names.contains("certifying_party_name"));
}

fn assert_regab_strategy_contract(strategy: &EntityStrategy) {
    assert_eq!(strategy.id, "regab_firm_identity.v1");
    assert_eq!(strategy.entity_type, "organization");
    assert_eq!(strategy.observations.name_fields, ["org_name"]);
    assert_eq!(
        strategy.observations.required_side_fields,
        [
            EntitySideField::AliasSurfacesJson,
            EntitySideField::MentionSurfacesJson
        ]
    );
    assert_eq!(
        strategy
            .observations
            .anchor_fields
            .get("cik")
            .map(String::as_str),
        Some("filing_cik")
    );
    assert_eq!(
        strategy
            .observations
            .anchor_fields
            .get("accession")
            .map(String::as_str),
        Some("accession")
    );
    assert!(
        strategy
            .observations
            .context_fields
            .contains(&"subject_role".to_string())
    );
    assert!(
        strategy
            .observations
            .context_fields
            .contains(&"platform_capacity".to_string())
    );
    assert_eq!(
        strategy
            .evidence
            .must_link
            .iter()
            .map(|operator| operator.op.as_str())
            .collect::<Vec<_>>(),
        ["registry_alias_match"]
    );
    assert!(
        strategy.evidence.cannot_link.is_empty(),
        "filing CIK/accession are document context, not Reg AB firm identity cannot-link evidence"
    );
    let exact_support = strategy
        .evidence
        .support
        .iter()
        .find(|operator| operator.op == "exact_view")
        .expect("exact_view support remains available for review evidence");
    assert_eq!(exact_support.params.get("score"), Some(&json!(8)));
    assert!(
        exact_support
            .params
            .get("score")
            .and_then(Value::as_i64)
            .is_some_and(|score| score < strategy.solver.backbone_score_min),
        "exact-name support must not form Reg AB firm merges without reviewed registry aliases"
    );
    assert!(strategy.anchors.trusted_for_must_link.is_empty());
    assert!(strategy.anchors.trusted_for_single_doc_promotion.is_empty());
    assert_eq!(strategy.anchors.support_only, ["accession", "cik"]);
}
