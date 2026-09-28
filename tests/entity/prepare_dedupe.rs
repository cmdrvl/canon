use canon::entity::prepare::{
    PrepareInputContract, PreparedSurfaceRecord, prepare_surface_records,
    project_prepare_csv_reader,
};
use canon::entity::profile::EntityProfileDocument;
use std::io::Cursor;

const CMBS_PROFILE: &str = include_str!("../fixtures/entity/profiles/cmbs_tenant_label.yaml");

#[test]
fn builtin_prepare_preserves_distinct_instrument_tuples_with_the_same_title() {
    let profile = EntityProfileDocument::from_yaml_str(include_str!(
        "../fixtures/entity/profiles/instrument_identity.yaml"
    ))
    .unwrap();
    let contract = PrepareInputContract::for_builtin_profile(&profile).unwrap();
    let input = concat!(
        "source_row_id,report_period,title,cusip,isin,maturitydt,annualizedrt,issuer_lei\n",
        "1,2026-Q1,ACME NOTE,111111111,US1111111111,2030-01-01,5,ISSUER\n",
        "2,2026-Q1,ACME NOTE,222222222,US2222222222,2031-01-01,5,ISSUER\n",
        "3,2026-Q1,ACME NOTE,111111111,US1111111111,2030-01-01,5,ISSUER\n",
    );
    let mut observations = project_prepare_csv_reader(Cursor::new(input), b',', &contract).unwrap();
    let surfaces = prepare_surface_records(&observations).unwrap();
    assert_eq!(surfaces.len(), 2);
    assert_ne!(surfaces[0].surface_id, surfaces[1].surface_id);
    assert_eq!(
        surfaces
            .iter()
            .map(|surface| surface.row_count)
            .sum::<u64>(),
        3
    );
    observations.reverse();
    assert_eq!(prepare_surface_records(&observations).unwrap(), surfaces);
}

#[test]
fn entity_prepare_dedupe() {
    let surfaces = surfaces_for(rows_in_original_order());
    assert_eq!(surfaces.len(), 2);

    let sears = surface(&surfaces, "cmbs_tenant_label:sears");
    assert!(
        sears
            .surface_id
            .starts_with("surf:cmbs_tenant_label:blake3:")
    );
    assert_eq!(sears.row_count, 3);
    assert_eq!(sears.deal_count, 2);
    assert_eq!(sears.normalized_views["tenant_core"].value, "sears");
    assert!(
        sears.normalized_views["tenant_core"]
            .reason_codes
            .contains(&"legal_suffix_stripped".to_string())
    );
    assert!(
        sears.normalized_views["tenant_tokens"]
            .reason_codes
            .contains(&"tokens_deduped".to_string())
            || sears.normalized_views["tenant_tokens"]
                .reason_codes
                .contains(&"source_parity_reference".to_string())
    );
    assert_eq!(sears.row_count, sears.provenance_samples.len() as u64);
    assert_eq!(
        sears
            .provenance_samples
            .iter()
            .map(|sample| sample.source_row_id.as_deref())
            .collect::<Vec<_>>(),
        [Some("row-1"), Some("row-2"), Some("row-3")]
    );
    assert!(sears.alias_surfaces.contains(&"Sears Roebuck".to_string()));

    let auto = surface(&surfaces, "cmbs_tenant_label:sears auto center");
    assert_eq!(auto.row_count, 1);
    assert_eq!(auto.deal_count, 1);
}

#[test]
#[allow(non_snake_case)]
fn EN_P001_duplicate_rows_collapse_to_unique_prepared_surfaces() {
    let surfaces = surfaces_for(rows_in_original_order());

    let mut surface_keys = surfaces
        .iter()
        .map(|surface| surface.surface_key.as_str())
        .collect::<Vec<_>>();
    surface_keys.sort();
    assert_eq!(
        surface_keys,
        [
            "cmbs_tenant_label:sears",
            "cmbs_tenant_label:sears auto center"
        ]
    );
    assert_eq!(
        surfaces
            .iter()
            .map(|surface| surface.row_count)
            .sum::<u64>(),
        4
    );
}

#[test]
fn entity_prepare_ordering() {
    let original = surfaces_for(rows_in_original_order());
    let reordered = surfaces_for(rows_in_reordered_input());

    assert_eq!(reordered, original);
}

fn surfaces_for(input: &str) -> Vec<PreparedSurfaceRecord> {
    let profile = EntityProfileDocument::from_yaml_str(CMBS_PROFILE).expect("valid profile");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");
    let observations =
        project_prepare_csv_reader(Cursor::new(input), b',', &contract).expect("prepare projects");
    prepare_surface_records(&observations).expect("surface records prepare")
}

fn surface<'a>(
    surfaces: &'a [PreparedSurfaceRecord],
    surface_key: &str,
) -> &'a PreparedSurfaceRecord {
    surfaces
        .iter()
        .find(|surface| surface.surface_key == surface_key)
        .expect("surface exists")
}

fn rows_in_original_order() -> &'static str {
    concat!(
        "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
        "row-1,deal-1,loan-1,prop-1,Sears LLC,\"[\"\"Sears Roebuck\"\"]\",[]\n",
        "row-2,deal-1,loan-2,prop-1,SEARS LLC,,[]\n",
        "row-3,deal-2,loan-3,prop-2,Sears,,[]\n",
        "row-4,deal-3,loan-4,prop-3,Sears Auto Center,,[]\n",
    )
}

fn rows_in_reordered_input() -> &'static str {
    concat!(
        "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
        "row-4,deal-3,loan-4,prop-3,Sears Auto Center,,[]\n",
        "row-3,deal-2,loan-3,prop-2,Sears,,[]\n",
        "row-2,deal-1,loan-2,prop-1,SEARS LLC,,[]\n",
        "row-1,deal-1,loan-1,prop-1,Sears LLC,\"[\"\"Sears Roebuck\"\"]\",[]\n",
    )
}
