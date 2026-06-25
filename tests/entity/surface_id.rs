use canon::entity::prepare::{
    PrepareInputContract, PreparedSurfaceRecord, prepare_surface_records,
    project_prepare_csv_reader,
};
use canon::entity::profile::EntityProfileDocument;
use std::io::Cursor;

const CMBS_PROFILE: &str = include_str!("../fixtures/entity/profiles/cmbs_tenant_label.yaml");

#[test]
fn entity_surface_id() {
    let surfaces = surfaces_for(rows_in_original_order());
    assert_eq!(surfaces.len(), 2);
    assert_sorted_by_surface_id(&surfaces);

    let sears = surface(&surfaces, "cmbs_tenant_label:sears");
    assert_eq!(sears.normalized_views["tenant_core"].value, "sears");
    assert_eq!(sears.row_count, 3);
    assert_eq!(sears.deal_count, 2);
    assert_surface_id_shape(sears);

    let auto = surface(&surfaces, "cmbs_tenant_label:sears auto center");
    assert_surface_id_shape(auto);
    assert_ne!(sears.surface_id, auto.surface_id);
}

#[test]
#[allow(non_snake_case)]
fn EN_P002_reordered_rows_keep_byte_identical_surface_ids_and_sorted_output() {
    let original = surfaces_for(rows_in_original_order());
    let reordered = surfaces_for(rows_in_reordered_input());

    assert_eq!(reordered, original);
    assert_sorted_by_surface_id(&original);
}

#[test]
fn surface_ids_do_not_include_source_row_ids() {
    let surfaces = surfaces_for(rows_in_original_order());

    for surface in surfaces {
        assert!(!surface.surface_id.contains("row-"));
        assert!(!surface.surface_id.contains("loan-"));
        assert!(!surface.surface_id.contains("prop-"));
    }
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

fn assert_surface_id_shape(surface: &PreparedSurfaceRecord) {
    let prefix = format!("surf:{}:blake3:", surface.profile_id);
    assert!(surface.surface_id.starts_with(&prefix));
    let digest = surface
        .surface_id
        .strip_prefix(&prefix)
        .expect("surface id has prefix");
    assert_eq!(digest.len(), 64);
    assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

fn assert_sorted_by_surface_id(surfaces: &[PreparedSurfaceRecord]) {
    assert!(
        surfaces
            .windows(2)
            .all(|pair| pair[0].surface_id < pair[1].surface_id)
    );
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
