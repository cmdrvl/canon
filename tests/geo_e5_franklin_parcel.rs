#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_QUESTION_VERSION, CANON_GEO_RESOURCE_BUDGET_VERSION,
    E5_FRANKLIN_AUDITOR_PARCELS_ADMITTED_ROWS, E5_FRANKLIN_AUDITOR_PARCELS_H3_COVERAGE_ROWS,
    E5_FRANKLIN_AUDITOR_PARCELS_SOURCE_INSTANCE_ID, E5_FRANKLIN_CURRENT_SUBJECT_PROPERTIES,
    E5_FRANKLIN_PARCEL_PIP_REACHED_PROPERTIES, E5_FRANKLIN_PARCEL_PIP_UNREACHED_PROPERTIES,
    E5_MICROSOFT_GLOBALML_FOOTPRINTS_ROWS, E5_MICROSOFT_GLOBALML_FOOTPRINTS_SOURCE_INSTANCE_ID,
    E5_MICROSOFT_GLOBALML_FRANKLIN_OCCUPIED_WORK_CELLS,
    E5_MICROSOFT_GLOBALML_FRANKLIN_THIN_TIER_FEATURES, GeoAbstentionDisposition,
    GeoAbstentionPolicy, GeoBudgetAction, GeoClaimClass, GeoControlEntityLevel,
    GeoE5FuelPrecisionStatus, GeoE5FuelReachKind, GeoE5FuelSourcePin, GeoEvidenceClass,
    GeoInventorySupportStatus, GeoNumericBound, GeoQuestion, GeoRequestedGrain, GeoResourceBudget,
    GeoResourceCounter, GeoSubjectBinding, GeoSubjectBindingClass, GeoValueOrigin,
    e5_franklin_county_inventory, e5_franklin_current_fuel_curve_report,
    evaluate_inventory_support, franklin_county_region,
};

const REACH_SQL: &str =
    include_str!("../scripts/geo_measurements/e5_franklin_county_parcel_candidate_reach.sql");
const GEOMETRY_SQL: &str =
    include_str!("../scripts/geo_measurements/e5_franklin_county_live_geometry_probe.sql");
const MICROSOFT_COVERAGE_SQL: &str =
    include_str!("../scripts/geo_measurements/e5_microsoft_globalml_franklin_h3_coverage.sql");
const MANIFEST: &str = include_str!("../scripts/geo_measurements/manifest.json");

#[test]
fn franklin_parcel_reach_is_pinned_bounded_and_truth_blind() {
    for required in [
        "'80d0ea39-a5aa-4c27-a8d7-f662a4507257'::TEXT AS bridge_build_id",
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
fn franklin_parcel_reach_is_manifest_registered_as_exact_candidate_reach() {
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).expect("manifest json");
    let measurement = manifest["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .find(|measurement| measurement["id"] == "e5_franklin_county_parcel_candidate_reach_v0")
        .expect("parcel reach measurement");
    assert_eq!(measurement["result_row_validation"], "exact_manifest_rows");
    assert_eq!(
        measurement["release_pins"]["bridge_build_id"],
        "80d0ea39-a5aa-4c27-a8d7-f662a4507257"
    );
    assert_eq!(
        measurement["expected_denominators"]["reached_properties"],
        147
    );
    assert_eq!(
        measurement["expected_denominators"]["unreached_properties"],
        4
    );
    assert_eq!(
        measurement["expected_sanity"]["row_contract"],
        "canon_geo_e5_franklin_parcel_candidate_reach.v0"
    );
}

#[test]
fn microsoft_globalml_coverage_is_current_bridge_bounded_and_source_only() {
    for required in [
        "'80d0ea39-a5aa-4c27-a8d7-f662a4507257'::TEXT AS bridge_build_id",
        "'OH'::TEXT AS state",
        "'2026-07-24'::DATE AS release_dt",
        "MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_H3_COVERAGE_HOT",
        "MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT",
        "w.h3_cell = c.h3_cell",
        "c.state = (SELECT state FROM params)",
        "COUNT(DISTINCT coverage_rows.provider_feature_id)",
        "features_without_hot_geometry",
        "COUNT(DISTINCT property_key) FROM subjects",
        "'canon_geo_e5_microsoft_globalml_franklin_h3_coverage.v0'",
    ] {
        assert!(
            MICROSOFT_COVERAGE_SQL.contains(required),
            "Microsoft coverage SQL must contain {required:?}"
        );
    }

    let folded = MICROSOFT_COVERAGE_SQL.to_ascii_lowercase();
    for forbidden in [
        "truth_bbl",
        "st_contains",
        "propertyaddress",
        "siteaddres",
        "result_scan",
        "limit 1",
        "precision as",
        "accuracy as",
    ] {
        assert!(
            !folded.contains(forbidden),
            "Microsoft coverage SQL must not use {forbidden:?}"
        );
    }
}

#[test]
fn microsoft_globalml_coverage_is_manifest_registered_as_exact_source_availability() {
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).expect("manifest json");
    let measurement = manifest["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .find(|measurement| measurement["id"] == "e5_microsoft_globalml_franklin_h3_coverage_v0")
        .expect("Microsoft coverage measurement");
    assert_eq!(measurement["result_row_validation"], "exact_manifest_rows");
    assert_eq!(
        measurement["release_pins"]["bridge_build_id"],
        "80d0ea39-a5aa-4c27-a8d7-f662a4507257"
    );
    assert_eq!(measurement["release_pins"]["state"], "OH");
    assert_eq!(
        measurement["release_pins"]["microsoft_globalml.release_dt"],
        "2026-07-24"
    );
    assert_eq!(
        measurement["expected_denominators"]["subject_properties"],
        151
    );
    assert_eq!(measurement["expected_denominators"]["work_cells"], 585);
    assert_eq!(
        measurement["expected_denominators"]["coverage_rows"],
        168778
    );
    assert_eq!(
        measurement["expected_denominators"]["occupied_work_cells"],
        581
    );
    assert_eq!(
        measurement["expected_sanity"]["row_contract"],
        "canon_geo_e5_microsoft_globalml_franklin_h3_coverage.v0"
    );
    assert_eq!(
        measurement["expected_sanity"]["features_without_hot_geometry"],
        0
    );
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
        include_str!("../src/geo/evaluation.rs"),
        include_str!("../src/geo/observer.rs"),
        include_str!("../src/geo/ledger.rs"),
        include_str!("../src/geo/materialize.rs"),
        include_str!("../src/geo/geometry_value.rs"),
        include_str!("../src/geo/evidence.rs"),
        include_str!("../src/geo/composition.rs"),
        include_str!("../src/geo/tile.rs"),
    ] {
        let folded = source.to_ascii_lowercase();
        for forbidden in [
            "franklin",
            "39049",
            "epsg:3735",
            "microsoft_globalml",
            "globalml",
        ] {
            assert!(
                !folded.contains(forbidden),
                "generic Geo module must not carry source-specific literal {forbidden:?}"
            );
        }
    }
}

#[test]
fn e5_fuel_instances_satisfy_generic_inventory_support_without_kernel_edits() {
    let region = franklin_county_region();
    let inventory = e5_franklin_county_inventory(
        "inventory.e5.franklin.typical_county",
        source_pin("franklin-parcel"),
        source_pin("microsoft-footprint"),
    );
    let budget = budget();

    let parcel_report = evaluate_inventory_support(
        &question(
            "question.e5.franklin.parcel_reach",
            region.clone(),
            GeoControlEntityLevel::Parcel,
            GeoEvidenceClass::ParcelGeometry,
            vec![GeoClaimClass::CandidateReach],
        ),
        &inventory,
        &budget,
    )
    .expect("parcel support report");
    assert_eq!(parcel_report.status, GeoInventorySupportStatus::Supported);
    assert_eq!(
        parcel_report.grain_support[0].satisfied_evidence_classes,
        vec![GeoEvidenceClass::ParcelGeometry]
    );
    assert!(parcel_report.discovery_gaps.is_empty());

    let footprint_report = evaluate_inventory_support(
        &question(
            "question.e5.franklin.building_footprints",
            region,
            GeoControlEntityLevel::Building,
            GeoEvidenceClass::BuildingFootprint,
            vec![GeoClaimClass::CandidateReach],
        ),
        &inventory,
        &budget,
    )
    .expect("building footprint support report");
    assert_eq!(
        footprint_report.status,
        GeoInventorySupportStatus::Supported
    );
    assert_eq!(
        footprint_report.grain_support[0].satisfied_evidence_classes,
        vec![GeoEvidenceClass::BuildingFootprint]
    );
    assert!(footprint_report.discovery_gaps.is_empty());
}

#[test]
fn e5_fuel_instances_carry_candidate_and_coverage_denominators() {
    let inventory = e5_franklin_county_inventory(
        "inventory.e5.franklin.denominators",
        source_pin("franklin-parcel"),
        source_pin("microsoft-footprint"),
    );
    let parcel_source = inventory
        .sources
        .iter()
        .find(|source| source.source_instance_id == E5_FRANKLIN_AUDITOR_PARCELS_SOURCE_INSTANCE_ID)
        .expect("Franklin parcel source");
    assert_eq!(
        estimate(parcel_source, "source.admitted_parcels"),
        E5_FRANKLIN_AUDITOR_PARCELS_ADMITTED_ROWS
    );
    assert_eq!(
        estimate(parcel_source, "source.h3_r8_coverage_rows"),
        E5_FRANKLIN_AUDITOR_PARCELS_H3_COVERAGE_ROWS
    );

    let footprint_source = inventory
        .sources
        .iter()
        .find(|source| {
            source.source_instance_id == E5_MICROSOFT_GLOBALML_FOOTPRINTS_SOURCE_INSTANCE_ID
        })
        .expect("Microsoft footprint source");
    assert_eq!(
        estimate(footprint_source, "source.global_footprint_rows"),
        E5_MICROSOFT_GLOBALML_FOOTPRINTS_ROWS
    );
    assert_eq!(
        estimate(footprint_source, "source.franklin_thin_tier_features"),
        E5_MICROSOFT_GLOBALML_FRANKLIN_THIN_TIER_FEATURES
    );
}

#[test]
fn e5_fuel_curve_report_separates_reach_from_waiting_precision() {
    let report = e5_franklin_current_fuel_curve_report();
    assert_eq!(report.tiers.len(), 2);
    assert!(report.required_core_dispatch_edits.is_empty());
    assert_eq!(report.frozen_nyc_e4_delta.resolved, 0);
    assert_eq!(report.frozen_nyc_e4_delta.correct, 0);
    assert_eq!(report.frozen_nyc_e4_delta.reach, 0);
    assert_eq!(report.frozen_nyc_e4_delta.false_merges, 0);

    let parcel = report
        .tiers
        .iter()
        .find(|tier| tier.tier_id == "franklin_parcel_candidate_reach")
        .expect("parcel reach tier");
    assert_eq!(parcel.evidence_class, GeoEvidenceClass::ParcelGeometry);
    assert_eq!(parcel.reach.kind, GeoE5FuelReachKind::CandidateReach);
    assert_eq!(
        parcel.reach.denominator_value,
        E5_FRANKLIN_CURRENT_SUBJECT_PROPERTIES
    );
    assert_eq!(
        parcel.reach.reached_properties,
        Some(E5_FRANKLIN_PARCEL_PIP_REACHED_PROPERTIES)
    );
    assert_eq!(
        parcel.reach.unreached_properties,
        Some(E5_FRANKLIN_PARCEL_PIP_UNREACHED_PROPERTIES)
    );
    assert_eq!(
        parcel.precision.status,
        GeoE5FuelPrecisionStatus::WaitingForTruthPlane
    );

    let footprint = report
        .tiers
        .iter()
        .find(|tier| tier.tier_id == "microsoft_globalml_footprint_h3_coverage")
        .expect("footprint source coverage tier");
    assert_eq!(
        footprint.evidence_class,
        GeoEvidenceClass::BuildingFootprint
    );
    assert_eq!(footprint.reach.kind, GeoE5FuelReachKind::SourceCoverage);
    assert_eq!(
        footprint.reach.denominator_value,
        E5_MICROSOFT_GLOBALML_FRANKLIN_THIN_TIER_FEATURES
    );
    assert_eq!(
        footprint.reach.occupied_work_cells,
        Some(E5_MICROSOFT_GLOBALML_FRANKLIN_OCCUPIED_WORK_CELLS)
    );
    assert_eq!(footprint.reach.reached_properties, None);
    assert_eq!(
        footprint.precision.status,
        GeoE5FuelPrecisionStatus::WaitingForTruthPlane
    );
}

#[test]
fn e5_fuel_instances_do_not_promote_evidence_only_sources_to_stable_aliases() {
    let region = franklin_county_region();
    let inventory = e5_franklin_county_inventory(
        "inventory.e5.franklin.evidence_only",
        source_pin("franklin-parcel"),
        source_pin("microsoft-footprint"),
    );
    let report = evaluate_inventory_support(
        &question(
            "question.e5.franklin.stable_identity",
            region,
            GeoControlEntityLevel::Parcel,
            GeoEvidenceClass::ParcelGeometry,
            vec![GeoClaimClass::CandidateReach, GeoClaimClass::StableIdentity],
        ),
        &inventory,
        &budget(),
    )
    .expect("stable identity support report");

    assert_eq!(report.status, GeoInventorySupportStatus::Unsupported);
    assert_eq!(
        report.grain_support[0].missing_evidence_classes,
        vec![GeoEvidenceClass::ParcelGeometry]
    );
    assert_eq!(report.discovery_gaps.len(), 1);
    assert!(
        report.discovery_gaps[0]
            .reason
            .contains("stable-alias participation"),
        "unexpected gap reason: {}",
        report.discovery_gaps[0].reason
    );
}

#[test]
fn e5_fuel_adapter_does_not_carry_nyc_identifier_assumptions() {
    let source = include_str!("../src/geo/e5_fuel.rs").to_ascii_lowercase();
    for forbidden in ["bbl", "mappluto", "borough", "pluto"] {
        assert!(
            !source.contains(forbidden),
            "E5 source adapters must not carry NYC identifier literal {forbidden:?}"
        );
    }
}

fn source_pin(seed: &str) -> GeoE5FuelSourcePin {
    GeoE5FuelSourcePin {
        release_digest: digest(&format!("{seed}:release")),
        local_artifact_id: format!("artifact.e5.{seed}"),
        local_content_hash: digest(&format!("{seed}:local")),
        geometry_transform_digest: Some(digest(&format!("{seed}:transform"))),
        lineage_ids: Vec::new(),
    }
}

fn question(
    question_id: &str,
    region: canon::geo::GeoBoundedGeography,
    entity_level: GeoControlEntityLevel,
    evidence_class: GeoEvidenceClass,
    requested_claim_classes: Vec<GeoClaimClass>,
) -> GeoQuestion {
    GeoQuestion {
        version: CANON_GEO_QUESTION_VERSION.to_string(),
        question_id: question_id.to_string(),
        subject_bindings: vec![GeoSubjectBinding {
            role: "operator_case".to_string(),
            binding_class: GeoSubjectBindingClass::OperatorLabel,
            value: question_id.to_string(),
        }],
        bounded_geography: region,
        requested_grains: vec![GeoRequestedGrain {
            entity_level,
            required_evidence_classes: vec![evidence_class],
            optional_evidence_classes: Vec::new(),
        }],
        query_as_of: None,
        requested_claim_classes,
        presentation_limits: vec![GeoNumericBound {
            semantic_id: "question.presentation.max_models".to_string(),
            counter: GeoResourceCounter::Models,
            value: 16,
            unit: "model".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
            action: GeoBudgetAction::TruncatePresentationOnly,
        }],
        abstention_policy: GeoAbstentionPolicy {
            unsupported_grain: GeoAbstentionDisposition::ReportUnsupported,
            unresolved_residual: GeoAbstentionDisposition::ReportResidual,
            budget_fallback: GeoAbstentionDisposition::ReportResidual,
        },
        decision_policy: None,
        resource_budget_ref: "budget.e5.franklin".to_string(),
    }
}

fn budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.e5.franklin".to_string(),
        deterministic_bounds: vec![GeoNumericBound {
            semantic_id: "budget.rows".to_string(),
            counter: GeoResourceCounter::Rows,
            value: 600_000,
            unit: "row".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
            action: GeoBudgetAction::RefuseBeforeWork,
        }],
        telemetry: Vec::new(),
    }
}

fn estimate(source: &canon::geo::GeoRegionalSourceInstance, semantic_id: &str) -> u64 {
    source
        .estimates
        .iter()
        .find(|estimate| estimate.semantic_id == semantic_id)
        .unwrap_or_else(|| panic!("missing estimate {semantic_id}"))
        .value
}

fn digest(seed: &str) -> String {
    format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex())
}
