#![forbid(unsafe_code)]

const SQL: &str = include_str!("../scripts/geo_measurements/h7_candidate_strategy_comparison.sql");

fn lower_sql() -> String {
    SQL.to_ascii_lowercase()
}

fn cte_offset(name: &str) -> usize {
    lower_sql()
        .find(&format!("{name} as"))
        .unwrap_or_else(|| panic!("missing CTE {name}"))
}

fn cte_body_before(name: &str, boundary: &str) -> String {
    let sql = lower_sql();
    let start = sql
        .find(&format!("{name} as"))
        .unwrap_or_else(|| panic!("missing CTE {name}"));
    let end = sql
        .find(&format!("{boundary} as"))
        .unwrap_or_else(|| panic!("missing boundary CTE {boundary}"));
    assert!(start < end, "{name} must appear before {boundary}");
    sql[start..end].to_string()
}

#[test]
fn candidate_strategy_comparison_is_point_only_without_address_channel() {
    let sql = lower_sql();
    for forbidden in [
        "propertyaddress",
        "property_address",
        "street_name",
        "street_number",
        "propertyzip",
        "property_zipcode",
        "propertycity",
    ] {
        assert!(
            !sql.contains(forbidden),
            "candidate comparison must not use {forbidden}"
        );
    }
    assert!(sql.contains("lip.latitude"));
    assert!(sql.contains("lip.longitude"));
    assert!(sql.contains("st_makepoint(lip.longitude, lip.latitude)"));
}

#[test]
fn candidate_relations_are_built_before_truth_is_flattened() {
    let truth_edges = cte_offset("truth_edges");
    for cte in [
        "h3_candidate_members",
        "pip_edges",
        "pip_blocks",
        "pip_candidate_members",
        "selector_candidate_members",
        "cascade_candidate_members",
        "candidate_members",
    ] {
        assert!(
            cte_offset(cte) < truth_edges,
            "{cte} must be constructed before truth_edges"
        );
    }
    assert!(SQL.contains("LATERAL FLATTEN(input => sr.truth_bbls)"));
    assert!(SQL.contains("COUNT(DISTINCT IFF(c.candidate_bbl IS NOT NULL, t.truth_bbl, NULL))"));
}

#[test]
fn candidate_ctes_do_not_seed_membership_from_truth_bbls() {
    for cte in [
        "h3_candidate_members",
        "pip_edges",
        "pip_blocks",
        "pip_candidate_members",
        "selector_candidate_members",
        "cascade_candidate_members",
        "candidate_members",
    ] {
        let body = cte_body_before(cte, "truth_edges");
        assert!(
            !body.contains("truth_bbl"),
            "{cte} must not inspect accepted truth BBLs"
        );
        assert!(
            !body.contains("legal_bbl"),
            "{cte} must not inspect legal BBL truth"
        );
    }
}

#[test]
fn selectors_share_the_same_subject_release_denominator() {
    let sql = lower_sql();
    assert!(sql.contains("71::number(38,0) as expected_accepted_subjects"));
    assert!(sql.contains("2::number(38,0) as expected_release_count"));
    assert!(sql.contains("from subject_releases sr"));
    assert!(sql.contains("cross join candidate_selectors cs"));
    assert!(
        sql.contains("selector_release_subjects = (select expected_accepted_subjects from params)")
    );
    assert!(sql.contains("as selector_denominator_guard"));
    assert!(sql.contains("as complete_denominator_guard"));
}

#[test]
fn comparison_preserves_selector_and_plane_boundaries() {
    let sql = lower_sql();
    for required in [
        "'h3_r8_k1'",
        "'pip_six_digit_bbl_block'",
        "'union_cascade_h3_then_pip_block'",
        "'union_cascade_reach_accounting_only'",
        "r.truth_plane",
        "r.association_plane",
        "r.release",
        "full_reach_subjects",
        "partial_reach_subjects",
        "no_reach_subjects",
        "reach_accounting_failures",
        "reached_lte_truth_guard",
        "union_cardinality_failures",
        "h3_pip_overlap_bbl_edges",
    ] {
        assert!(sql.contains(required), "missing {required}");
    }
}
