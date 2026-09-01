#![forbid(unsafe_code)]

const REACH_SQL: &str =
    include_str!("../scripts/geo_measurements/e5_franklin_county_parcel_candidate_reach.sql");
const GEOMETRY_SQL: &str =
    include_str!("../scripts/geo_measurements/e5_franklin_county_live_geometry_probe.sql");

#[test]
fn franklin_parcel_reach_is_pinned_bounded_and_truth_blind() {
    for required in [
        "'ce3953ac-c2d4-4b48-bf02-29f0cf341389'::TEXT AS bridge_build_id",
        "'hub-de09f99cce0bcae7142d6d2e26582fd3-25'::TEXT AS parcel_release",
        "'2026-09-01'::DATE AS parcel_release_dt",
        "FRANKLIN_COUNTY_AUDITOR_PARCELS_FEATURE_H3_COVERAGE",
        "c.h3_cell = s.point_h3_r8",
        "source_geometry_validity = 'valid'",
        "source_geom_wkb_sha256 IS NOT NULL",
        "geom_wgs84_sha256 IS NOT NULL",
        "ST_CONTAINS(p.geom_geog, c.point_geog)",
        "stats.reached_properties + stats.unreached_properties",
        "stats.unique_pip_properties + stats.multi_pip_properties",
        "miss_stats.diagnosed_misses = stats.unreached_properties",
    ] {
        assert!(
            REACH_SQL.contains(required),
            "reach SQL must contain {required:?}"
        );
    }

    let folded = REACH_SQL.to_ascii_lowercase();
    for forbidden in [
        "propertyaddress",
        "siteaddres",
        "truth_bbl",
        "salecount",
        "statedarea /",
        "statedarea)",
        "limit 1",
        "result_scan",
    ] {
        assert!(
            !folded.contains(forbidden),
            "reach SQL must not use {forbidden:?}"
        );
    }
}

#[test]
fn live_geometry_probe_is_seeded_source_byte_bound_and_cli_shaped() {
    for required in [
        "'canon-e5-franklin-live-geometry-2026-09-01-v0'::TEXT AS selection_seed",
        "SHA2_HEX(",
        "ORDER BY selection_rank, property_key, provider_feature_id",
        "BASE64_DECODE_BINARY(chosen.source_geom_wkb)",
        "= chosen.source_geom_wkb_sha256",
        "'version', 'canon_geo_warehouse_geometry_rows.v0'",
        "'source_unit_to_millimetres'",
        "'unit_id', 'us-survey-foot'",
        "'numerator', 1200000",
        "'denominator', 3937",
        "'source_geom_wkb_base64', chosen.source_geom_wkb",
        "'transform_execution_id', chosen.transform_execution_id",
        "e.source_vertex_count <= (SELECT max_vertices_per_geometry FROM params)",
    ] {
        assert!(
            GEOMETRY_SQL.contains(required),
            "geometry SQL must contain {required:?}"
        );
    }

    let folded = GEOMETRY_SQL.to_ascii_lowercase();
    for forbidden in [
        "44f1ae2b-afd0-40ca-9eb4-26eae5e7f982",
        "crep-04ee1fb6dca66d9a",
        "propertyaddress",
        "truth_bbl",
        "statedarea",
        "order by e.source_vertex_count",
        "result_scan",
    ] {
        assert!(
            !folded.contains(forbidden),
            "geometry SQL must not depend on {forbidden:?}"
        );
    }
}

#[test]
fn franklin_instance_names_do_not_enter_the_generic_geo_engine() {
    for source in [
        include_str!("../src/geo/materialize.rs"),
        include_str!("../src/geo/geometry_value.rs"),
        include_str!("../src/geo/composition.rs"),
        include_str!("../src/geo/tile.rs"),
    ] {
        let folded = source.to_ascii_lowercase();
        assert!(!folded.contains("franklin"));
        assert!(!folded.contains("39049"));
        assert!(!folded.contains("epsg:3735"));
    }
}
