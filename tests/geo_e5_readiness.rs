#![forbid(unsafe_code)]

const SQL: &str =
    include_str!("../scripts/geo_measurements/e5_franklin_county_thin_tier_readiness.sql");

#[test]
fn e5_readiness_is_bounded_release_pinned_and_address_blind() {
    for required in [
        "'39049'::TEXT AS county_fips",
        "'3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id",
        "8::NUMBER(9,0) AS h3_resolution",
        "1::NUMBER(9,0) AS halo_k",
        "('fema_structures', 'fema', 'fema_usa_structures_hot'",
        "'2023-05-02'",
        "('microsoft_footprints', 'microsoft'",
        "'2026-07-24'",
        "('overture_addresses', 'overture_maps'",
        "('overture_buildings', 'overture_maps'",
        "'2026-07-22.0'",
        "H3_GRID_DISK(",
        "keys.h3_r8_int IN (SELECT h3_r8_int FROM work_cells)",
        "required_source_empty:",
        "source_row_duplicate_inflation:",
    ] {
        assert!(SQL.contains(required), "SQL must contain {required:?}");
    }

    let folded = SQL.to_ascii_lowercase();
    for forbidden in [
        "propertyaddress",
        "nyc_",
        "mappluto",
        "truth_bbl",
        "result_scan",
        "st_intersects",
        "solve_composition",
    ] {
        assert!(
            !folded.contains(forbidden),
            "readiness SQL must not depend on {forbidden:?}"
        );
    }
}

#[test]
fn e5_readiness_keeps_source_count_out_of_evidence_weight() {
    assert!(SQL.contains("COUNT(DISTINCT keys.provider_feature_id) AS distinct_features"));
    assert!(SQL.contains("COUNT(DISTINCT keys.h3_r8_int) AS occupied_work_cells"));
    assert!(!SQL.contains("evidence_weight"));
    assert!(!SQL.contains("confidence_score"));
    assert!(!SQL.contains("SUM(feature_rows)"));
}
