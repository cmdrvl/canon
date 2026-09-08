#![forbid(unsafe_code)]

use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    fs::File,
    io::BufReader,
};

const ARTIFACT_PATH: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/pluto_vintage_reach_measurement.json";
const POPULATION_PATH: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03/population_request_roll_universe.json.gz";
const SQL_PATH: &str = "scripts/geo_measurements/e4_pluto_vintage_reach.sql";
const CONDO_BILLING_GEOMETRY_PATH: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/condo_billing_geometry_bridge_measurement.json";
const CONDO_BILLING_GEOMETRY_SQL_PATH: &str =
    "scripts/geo_measurements/e4_condo_billing_geometry_bridge.sql";
const WIDENED_POPULATION_PATH: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_widened.json.gz";
const CONDO_REPRESENTATION_REACH_PATH: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/condo_representation_reach_measurement.json";
const CONDO_REPRESENTATION_WIDENED_POPULATION_PATH: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_condo_representation_widened.json.gz";

#[derive(Debug, Deserialize)]
struct PopulationRequest {
    cases: Vec<PopulationCase>,
}

#[derive(Debug, Deserialize)]
struct PopulationCase {
    id: String,
    truth_plane: String,
    evidence: EvidenceRequest,
    truth: TruthSet,
}

#[derive(Debug, Deserialize)]
struct EvidenceRequest {
    universe: Universe,
}

#[derive(Debug, Deserialize)]
struct Universe {
    parcels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TruthSet {
    parcels: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ReachCounts {
    full: u64,
    partial: u64,
    none: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaseChange {
    truth_plane: String,
    before_reach: String,
    after_reach: String,
    newly_reached_truth_bbls: Vec<String>,
    still_missing_truth_bbl_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResidualCase {
    truth_plane: String,
    after_reach: String,
    still_missing_truth_bbl_count: u64,
    first_still_missing_truth_bbl: String,
    last_still_missing_truth_bbl: String,
}

#[test]
fn pluto_vintage_reach_measurement_recomputes_frozen_70_delta() {
    let artifact = read_json(ARTIFACT_PATH);
    assert_eq!(
        artifact["version"],
        "canon_geo_e4_pluto_vintage_reach_measurement.v0"
    );
    assert_eq!(artifact["bead"], "bd-1q5y");
    assert_eq!(
        artifact["proof_class"],
        "retained_cmdrvl_data_warehouse_measurement_not_live"
    );
    assert!(
        artifact["boundary"]
            .as_str()
            .expect("boundary string")
            .contains("Candidate-universe widening only"),
        "measurement must not present candidate reach as collateral truth"
    );

    let population = read_population();
    assert_eq!(population.cases.len(), 70);
    let historical_bbls = pluto_hit_bbls(&artifact);
    assert_eq!(historical_bbls.len(), 31);
    assert!(pluto_hits_are_historical_only(&artifact));

    let derived = derive_reach(&population, &historical_bbls);
    assert_eq!(
        derived.before,
        ReachCounts {
            full: 47,
            partial: 15,
            none: 8
        }
    );
    assert_eq!(
        derived.after,
        ReachCounts {
            full: 65,
            partial: 2,
            none: 3
        }
    );
    assert_eq!(derived.missing_truth_bbl_refs, 371);
    assert_eq!(derived.unique_missing_truth_bbls, 362);
    assert_eq!(derived.changed_cases.len(), 18);
    assert_eq!(derived.residual_cases.len(), 5);

    assert_reach_counts(
        &artifact["frozen_population"]["initial_reach"],
        &derived.before,
    );
    assert_reach_counts(&artifact["reach_measurement"]["before"], &derived.before);
    assert_reach_counts(&artifact["reach_measurement"]["after"], &derived.after);
    assert_eq!(
        required_u64(&artifact["frozen_population"]["case_denominator"]),
        70
    );
    assert_eq!(
        required_u64(&artifact["frozen_population"]["frozen_gate_denominator"]),
        79
    );
    assert_eq!(
        required_u64(&artifact["frozen_population"]["missing_truth_bbl_refs"]),
        derived.missing_truth_bbl_refs
    );
    assert_eq!(
        required_u64(&artifact["frozen_population"]["unique_missing_truth_bbls"]),
        derived.unique_missing_truth_bbls
    );
    assert_eq!(
        required_u64(&artifact["reach_measurement"]["subjects_recovered_to_full_reach"]),
        18
    );
    assert_eq!(
        required_u64(&artifact["reach_measurement"]["candidate_recall_failure_cases_after"]),
        5
    );
    assert_eq!(
        required_u64(&artifact["reach_measurement"]["present_current_unique_missing_bbls"]),
        0
    );
    assert_eq!(
        required_u64(&artifact["reach_measurement"]["historical_only_unique_missing_bbls"]),
        31
    );

    assert_changed_cases(&artifact, &derived.changed_cases);
    assert_residual_cases(&artifact, &derived.residual_cases);
    assert_widened_population_artifact(&artifact, &population, &historical_bbls, &derived.after);
    assert_source_pins_and_sql_boundary(&artifact);
}

#[test]
fn remaining_five_condo_billing_geometry_bridge_keeps_unit_absence_explicit() {
    let artifact = read_json(CONDO_BILLING_GEOMETRY_PATH);
    assert_eq!(
        artifact["version"],
        "canon_geo_e4_condo_billing_geometry_bridge_measurement.v0"
    );
    assert_eq!(artifact["bead"], "bd-1q5y");
    assert_eq!(
        artifact["proof_class"],
        "retained_cmdrvl_data_warehouse_measurement_not_live"
    );
    let boundary = artifact["boundary"].as_str().expect("boundary string");
    assert!(boundary.contains("Candidate-universe and representation-bridge measurement only"));
    assert!(boundary.contains("not unit-lot geometry"));
    assert!(boundary.contains("not collateral truth"));
    assert!(boundary.contains("does not relax rho admission"));

    assert_eq!(
        artifact["input_artifacts"]["widened_population_sha256"]
            .as_str()
            .expect("widened population SHA256"),
        sha256_hex(
            &fs::read(WIDENED_POPULATION_PATH).expect("read widened population for bridge check")
        )
    );
    assert_reach_counts(
        &artifact["frozen_population"]["pluto_vintage_reach_after"],
        &ReachCounts {
            full: 65,
            partial: 2,
            none: 3,
        },
    );
    assert_eq!(
        required_u64(&artifact["frozen_population"]["residual_cases_after_pluto_vintage"]),
        5
    );
    assert_eq!(
        required_u64(&artifact["frozen_population"]["residual_unit_bbls_after_pluto_vintage"]),
        331
    );

    assert_condo_pad_bridge_summary(&artifact);
    assert_condo_geometry_summary(&artifact);
    assert_condo_case_bridge_rows(&artifact);
    assert_condo_geometry_row_pins(&artifact);
    assert_condo_bridge_sql_boundary();

    let interpretation = &artifact["reach_interpretation"];
    assert_eq!(
        interpretation["exact_unit_lot_geometry_status"]
            .as_str()
            .expect("exact unit status"),
        "absent_from_landed_mappluto_geometry"
    );
    assert_eq!(
        interpretation["bridgeable_candidate_geometry_status"]
            .as_str()
            .expect("bridgeable status"),
        "pad_billing_bbl_crosswalk_supplies_mappluto_billing_lot_geometry_for_all_residual_unit_lots"
    );
    assert!(
        interpretation["purchase_order_if_exact_unit_geometry_is_required"]
            .as_str()
            .expect("purchase-order text")
            .contains("DTM/condo tax-map geometry source"),
        "exact unit-lot geometry absence must name the acquisition needed"
    );
    assert_eq!(
        artifact["e4_solver_rescore"]["status"]
            .as_str()
            .expect("rescore status"),
        "not_scored_in_this_increment"
    );
}

#[test]
fn remaining_five_condo_representation_widening_recovers_reach_without_claiming_unit_geometry() {
    let artifact = read_json(CONDO_REPRESENTATION_REACH_PATH);
    assert_eq!(
        artifact["version"],
        "canon_geo_e4_condo_representation_reach_measurement.v0"
    );
    assert_eq!(artifact["bead"], "bd-1q6h");
    assert_eq!(
        artifact["proof_class"],
        "retained_cmdrvl_data_warehouse_measurement_with_bounded_mcp_replay_not_live_receipt"
    );
    let boundary = artifact["boundary"].as_str().expect("boundary string");
    assert!(boundary.contains("Candidate-universe-only recovery"));
    assert!(boundary.contains("typed representation bridge"));
    assert!(boundary.contains("not unit-lot geometry"));
    assert!(boundary.contains("not collateral truth"));
    assert!(boundary.contains("does not relax rho admission"));

    assert_eq!(
        artifact["answer"]["can_pad_crosswalk_plus_mappluto_supply_usable_candidate_geometry"]
            .as_str()
            .expect("answer status"),
        "yes_with_declared_condo_unit_to_billing_lot_representation"
    );
    assert_eq!(
        artifact["answer"]["answer_grain"]
            .as_str()
            .expect("answer grain"),
        "billing_lot_representation_grain"
    );
    assert!(
        artifact["answer"]["grain_caveat"]
            .as_str()
            .expect("grain caveat")
            .contains("not condo unit-lot grain"),
        "billing-lot representation must not be projected as unit-lot truth"
    );
    assert_eq!(
        artifact["answer"]["exact_unit_lot_geometry"]
            .as_str()
            .expect("exact unit-lot answer"),
        "no_landed_mappluto_geometry_rows_for_the_331_unit_bbls"
    );

    assert_eq!(
        artifact["input_artifacts"]["pluto_vintage_widened_population_sha256"]
            .as_str()
            .expect("input population SHA256"),
        sha256_hex(
            &fs::read(WIDENED_POPULATION_PATH).expect("read PLUTO-vintage widened population")
        )
    );
    assert_eq!(
        artifact["input_artifacts"]["condo_billing_geometry_bridge_sha256"]
            .as_str()
            .expect("bridge artifact SHA256"),
        sha256_hex(&fs::read(CONDO_BILLING_GEOMETRY_PATH).expect("read bridge artifact"))
    );

    assert_reach_counts(
        &artifact["frozen_population"]["reach_before_condo_representation_bridge"],
        &ReachCounts {
            full: 65,
            partial: 2,
            none: 3,
        },
    );
    assert_reach_counts(
        &artifact["frozen_population"]["reach_after_condo_representation_bridge"],
        &ReachCounts {
            full: 70,
            partial: 0,
            none: 0,
        },
    );
    assert_eq!(
        required_u64(&artifact["frozen_population"]["represented_candidate_unit_bbls_added"]),
        331
    );
    assert_eq!(
        required_u64(&artifact["frozen_population"]["residual_cases_after_bridge"]),
        0
    );

    let base = read_population_from(WIDENED_POPULATION_PATH);
    let recovered = read_population_from(CONDO_REPRESENTATION_WIDENED_POPULATION_PATH);
    let before_reach = derive_reach(&base, &BTreeSet::new());
    let after_reach = derive_reach(&recovered, &BTreeSet::new());
    assert_eq!(
        before_reach.before,
        ReachCounts {
            full: 65,
            partial: 2,
            none: 3,
        }
    );
    assert_eq!(
        after_reach.before,
        ReachCounts {
            full: 70,
            partial: 0,
            none: 0,
        }
    );

    assert_condo_representation_population_artifact(&artifact, &base, &recovered);
    assert_condo_representation_bridge_replay(&artifact);
    assert_eq!(
        artifact["candidate_population"]["sha256"]
            .as_str()
            .expect("candidate population SHA256"),
        sha256_hex(
            &fs::read(CONDO_REPRESENTATION_WIDENED_POPULATION_PATH)
                .expect("read representation-widened population")
        )
    );
    assert_eq!(
        artifact["candidate_population"]["status"]
            .as_str()
            .expect("candidate population status"),
        "ready_for_e4_scorer"
    );
    assert_eq!(
        artifact["e4_solver_rescore"]["status"]
            .as_str()
            .expect("E4 rescore status"),
        "not_scored_by_bd_1q6h"
    );
}

struct DerivedReach {
    before: ReachCounts,
    after: ReachCounts,
    missing_truth_bbl_refs: u64,
    unique_missing_truth_bbls: u64,
    changed_cases: BTreeMap<String, CaseChange>,
    residual_cases: BTreeMap<String, ResidualCase>,
}

fn derive_reach(
    population: &PopulationRequest,
    historical_bbls: &BTreeSet<String>,
) -> DerivedReach {
    let mut before = ReachCounts::default();
    let mut after = ReachCounts::default();
    let mut missing_truth_bbl_refs = 0_u64;
    let mut unique_missing_truth_bbls = BTreeSet::new();
    let mut changed_cases = BTreeMap::new();
    let mut residual_cases = BTreeMap::new();

    for case in &population.cases {
        let truth: BTreeSet<_> = case.truth.parcels.iter().cloned().collect();
        let universe: BTreeSet<_> = case.evidence.universe.parcels.iter().cloned().collect();
        let missing_before: BTreeSet<_> = truth.difference(&universe).cloned().collect();
        missing_truth_bbl_refs += u64::try_from(missing_before.len()).expect("missing count fits");
        unique_missing_truth_bbls.extend(missing_before.iter().cloned());

        let before_reach = reach_status(truth.intersection(&universe).count(), truth.len());
        before.increment(before_reach);

        let newly_reached_truth_bbls: Vec<_> = missing_before
            .intersection(historical_bbls)
            .cloned()
            .collect();
        let mut widened_universe = universe.clone();
        widened_universe.extend(newly_reached_truth_bbls.iter().cloned());
        let after_reach = reach_status(truth.intersection(&widened_universe).count(), truth.len());
        after.increment(after_reach);
        let still_missing: Vec<_> = truth.difference(&widened_universe).cloned().collect();

        if before_reach != after_reach || !newly_reached_truth_bbls.is_empty() {
            changed_cases.insert(
                case.id.clone(),
                CaseChange {
                    truth_plane: case.truth_plane.clone(),
                    before_reach: before_reach.to_string(),
                    after_reach: after_reach.to_string(),
                    newly_reached_truth_bbls,
                    still_missing_truth_bbl_count: u64::try_from(still_missing.len())
                        .expect("missing count fits"),
                },
            );
        }

        if after_reach != "full" {
            residual_cases.insert(
                case.id.clone(),
                ResidualCase {
                    truth_plane: case.truth_plane.clone(),
                    after_reach: after_reach.to_string(),
                    still_missing_truth_bbl_count: u64::try_from(still_missing.len())
                        .expect("missing count fits"),
                    first_still_missing_truth_bbl: still_missing
                        .first()
                        .expect("residual has missing truth")
                        .clone(),
                    last_still_missing_truth_bbl: still_missing
                        .last()
                        .expect("residual has missing truth")
                        .clone(),
                },
            );
        }
    }

    DerivedReach {
        before,
        after,
        missing_truth_bbl_refs,
        unique_missing_truth_bbls: u64::try_from(unique_missing_truth_bbls.len())
            .expect("unique count fits"),
        changed_cases,
        residual_cases,
    }
}

impl ReachCounts {
    fn increment(&mut self, status: &str) {
        match status {
            "full" => self.full += 1,
            "partial" => self.partial += 1,
            "none" => self.none += 1,
            unexpected => panic!("unexpected reach status {unexpected}"),
        }
    }
}

fn reach_status(reached: usize, truth: usize) -> &'static str {
    if reached == 0 {
        "none"
    } else if reached == truth {
        "full"
    } else {
        "partial"
    }
}

fn assert_changed_cases(artifact: &Value, expected: &BTreeMap<String, CaseChange>) {
    let actual = artifact["changed_cases"]
        .as_array()
        .expect("changed_cases array");
    assert_eq!(actual.len(), expected.len());
    for row in actual {
        let case_id = row["case_id"].as_str().expect("case id");
        let expected_row = expected.get(case_id).expect("changed case is derived");
        assert_eq!(
            row["truth_plane"].as_str().expect("truth plane"),
            expected_row.truth_plane.as_str()
        );
        assert_eq!(
            row["before_reach"].as_str().expect("before reach"),
            expected_row.before_reach.as_str()
        );
        assert_eq!(
            row["after_reach"].as_str().expect("after reach"),
            expected_row.after_reach.as_str()
        );
        assert_eq!(
            string_array(&row["newly_reached_truth_bbls"]),
            expected_row.newly_reached_truth_bbls
        );
        assert_eq!(
            required_u64(&row["still_missing_truth_bbl_count"]),
            expected_row.still_missing_truth_bbl_count
        );
    }
}

fn assert_residual_cases(artifact: &Value, expected: &BTreeMap<String, ResidualCase>) {
    let actual = artifact["residual_unreached_cases"]
        .as_array()
        .expect("residual_unreached_cases array");
    assert_eq!(actual.len(), expected.len());
    for row in actual {
        let case_id = row["case_id"].as_str().expect("case id");
        let expected_row = expected.get(case_id).expect("residual case is derived");
        assert_eq!(
            row["truth_plane"].as_str().expect("truth plane"),
            expected_row.truth_plane.as_str()
        );
        assert_eq!(
            row["after_reach"].as_str().expect("after reach"),
            expected_row.after_reach.as_str()
        );
        assert_eq!(
            required_u64(&row["still_missing_truth_bbl_count"]),
            expected_row.still_missing_truth_bbl_count
        );
        assert_eq!(
            row["first_still_missing_truth_bbl"]
                .as_str()
                .expect("first still missing truth BBL"),
            expected_row.first_still_missing_truth_bbl.as_str()
        );
        assert_eq!(
            row["last_still_missing_truth_bbl"]
                .as_str()
                .expect("last still missing truth BBL"),
            expected_row.last_still_missing_truth_bbl.as_str()
        );
        assert_eq!(
            row["cause"].as_str().expect("cause"),
            "pad_confirmed_condo_unit_lot_without_pluto_geometry"
        );
    }

    let pad_probe = &artifact["residual_pad_probe"];
    assert_eq!(required_u64(&pad_probe["residual_bbls"]), 331);
    assert_eq!(required_u64(&pad_probe["residual_bbls_in_pad_26b"]), 331);
    assert_eq!(
        required_u64(&pad_probe["residual_bbls_with_billing_bbl"]),
        331
    );
    assert_eq!(
        required_u64(&pad_probe["residual_bbls_in_condo_range"]),
        325
    );
    assert_string_array(
        &pad_probe["billing_bbls"],
        &[
            "1000287502",
            "1010297502",
            "1012747504",
            "1013267501",
            "4050147502",
            "4067977503",
        ],
    );
    assert_eq!(
        pad_probe["source_zip_sha256"]
            .as_str()
            .expect("PAD source zip SHA256"),
        "016a29968b4bed9e8dde10b9c27b68132aba994baf1dc3e2543a861eadfdf4bd"
    );
    assert_eq!(
        pad_probe["parser_version"]
            .as_str()
            .expect("PAD parser version"),
        "2026-08-16"
    );
}

fn assert_widened_population_artifact(
    artifact: &Value,
    original: &PopulationRequest,
    historical_bbls: &BTreeSet<String>,
    expected_reach: &ReachCounts,
) {
    assert_eq!(
        artifact["frozen_population"]["widened_population_path"]
            .as_str()
            .expect("widened population path"),
        WIDENED_POPULATION_PATH
    );
    let widened_bytes = fs::read(WIDENED_POPULATION_PATH).expect("read widened population bytes");
    assert_eq!(
        sha256_hex(&widened_bytes),
        artifact["frozen_population"]["widened_population_sha256"]
            .as_str()
            .expect("widened population SHA256")
    );

    let widened = read_population_from(WIDENED_POPULATION_PATH);
    assert_eq!(widened.cases.len(), original.cases.len());
    for (before, after) in original.cases.iter().zip(widened.cases.iter()) {
        assert_eq!(after.id, before.id);
        assert_eq!(after.truth_plane, before.truth_plane);
        assert_eq!(after.truth.parcels, before.truth.parcels);

        let before_universe: BTreeSet<_> =
            before.evidence.universe.parcels.iter().cloned().collect();
        let mut expected_universe = before_universe.clone();
        expected_universe.extend(
            before
                .truth
                .parcels
                .iter()
                .filter(|bbl| !before_universe.contains(*bbl) && historical_bbls.contains(*bbl))
                .cloned(),
        );
        let after_universe: BTreeSet<_> = after.evidence.universe.parcels.iter().cloned().collect();
        assert_eq!(
            after_universe, expected_universe,
            "widened universe drifted for {}",
            before.id
        );
    }

    let widened_reach = derive_reach(&widened, &BTreeSet::new());
    assert_eq!(&widened_reach.before, expected_reach);
}

fn assert_source_pins_and_sql_boundary(artifact: &Value) {
    let manifest = artifact["pluto_manifest_release_pins"]
        .as_array()
        .expect("manifest release pins");
    assert_eq!(manifest.len(), 12);
    for row in manifest {
        let sha = row["source_zip_sha256"].as_str().expect("source zip sha");
        assert_eq!(sha.len(), 64);
        assert!(sha.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    let tables = artifact["source_tables"]
        .as_array()
        .expect("source tables array");
    let pluto = tables
        .iter()
        .find(|row| row["table"] == "EDGAR_DB.SOURCE.NYC_DCP_PLUTO_LOT_VINTAGES")
        .expect("PLUTO source table pin");
    assert_string_array(
        &pluto["row_pins"],
        &[
            "RELEASE",
            "RELEASE_DT",
            "SOURCE_ROW_NUMBER",
            "SOURCE_FILENAME",
        ],
    );
    assert!(
        pluto["content_hash_source"]
            .as_str()
            .expect("content hash source")
            .contains("NYC_DCP_PLUTO_MANIFEST_EXT")
    );
    let pad = tables
        .iter()
        .find(|row| row["table"] == "EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT")
        .expect("PAD source table pin");
    assert!(
        string_array(&pad["row_pins"])
            .iter()
            .any(|pin| pin == "SOURCE_ZIP_SHA256"),
        "PAD attribution must retain its source ZIP content hash"
    );

    let sql = fs::read_to_string(SQL_PATH).expect("read PLUTO vintage SQL");
    assert!(sql.contains("JOIN probe_releases"));
    assert!(sql.contains("REGEXP_REPLACE(v.bbl, '\\\\.00$', '')"));
    assert!(sql.contains("p.release = '26B'"));
    assert!(sql.contains("historical_only_unique_missing_bbls"));
    assert!(
        !sql.contains("is_current_release = TRUE"),
        "the reach probe must not collapse to current-release-only PLUTO"
    );
}

fn assert_condo_pad_bridge_summary(artifact: &Value) {
    let pad = &artifact["pad_bridge_summary"];
    assert_eq!(pad["release"].as_str().expect("PAD release"), "26B");
    assert_eq!(
        pad["release_dt"].as_str().expect("PAD release date"),
        "2026-05-01"
    );
    assert_eq!(required_u64(&pad["residual_unit_bbls"]), 331);
    assert_eq!(required_u64(&pad["distinct_residual_unit_bbls"]), 331);
    assert_eq!(required_u64(&pad["units_with_pad_row"]), 331);
    assert_eq!(required_u64(&pad["units_with_billing_bbl"]), 331);
    assert_eq!(required_u64(&pad["units_matched_by_exact_key"]), 6);
    assert_eq!(required_u64(&pad["units_matched_by_range"]), 325);
    assert_eq!(required_u64(&pad["distinct_billing_bbls"]), 6);
    assert_string_array(
        &pad["billing_bbls"],
        &[
            "1000287502",
            "1010297502",
            "1012747504",
            "1013267501",
            "4050147502",
            "4067977503",
        ],
    );
    assert_eq!(
        pad["source_zip_sha256"].as_str().expect("PAD source hash"),
        "016a29968b4bed9e8dde10b9c27b68132aba994baf1dc3e2543a861eadfdf4bd"
    );
    assert_eq!(
        pad["parser_version"].as_str().expect("PAD parser version"),
        "2026-08-16"
    );
    assert!(
        pad["license_terms"]
            .as_str()
            .expect("PAD license terms")
            .contains("DCP disclaims")
    );
    assert_eq!(
        pad["attribution_text"].as_str().expect("PAD attribution"),
        "NYC Department of City Planning (DCP)"
    );
}

fn assert_condo_geometry_summary(artifact: &Value) {
    let geometry = &artifact["geometry_summary"];
    assert_eq!(
        required_u64(&geometry["unit_bbls_with_mappluto_geom_v3"]),
        0
    );
    assert_eq!(required_u64(&geometry["unit_bbl_geom_v3_rows"]), 0);
    assert_eq!(
        required_u64(&geometry["billing_bbls_with_mappluto_geom_v3"]),
        6
    );
    assert_eq!(required_u64(&geometry["mappluto_geom_v3_billing_rows"]), 12);
    assert_string_array(
        &geometry["mappluto_geom_v3_billing_releases"],
        &["26v1", "26v2"],
    );
    assert_eq!(
        required_u64(&geometry["mappluto_geom_v3_rows_with_source_archive_sha256"]),
        12
    );
    assert_eq!(
        required_u64(&geometry["mappluto_geom_v3_rows_with_geom_wgs84_sha256"]),
        12
    );
    assert_eq!(
        required_u64(&geometry["mappluto_geom_v3_rows_with_source_geom_wkb_sha256"]),
        12
    );
    assert_eq!(
        required_u64(&geometry["mappluto_geom_v3_rows_with_transform_execution_id"]),
        12
    );
}

fn assert_condo_case_bridge_rows(artifact: &Value) {
    let rows = artifact["case_bridge_rows"]
        .as_array()
        .expect("case bridge rows");
    assert_eq!(rows.len(), 5);
    let mut expected = BTreeMap::new();
    expected.insert(
        "h7-subject:non-round:655a127dc453d12220696e0a0f76929d2ab8dd91ebd1dbe363173e50408fb34b",
        (145_u64, "partial", vec!["4067977503"]),
    );
    expected.insert(
        "h7-subject:non-round:68a1a4ced3f7c16edc483b877013238d0e2c26692c9ef0a5a721e6bb25872612",
        (2_u64, "none", vec!["1012747504", "1013267501"]),
    );
    expected.insert(
        "h7-subject:non-round:e3a5228d84bb6bf01ff03e2849e4a996f223cb1fcff79eff04a65d84dcfa8deb",
        (10_u64, "none", vec!["4050147502"]),
    );
    expected.insert(
        "h7-subject:round-exact-lender:0a6ff10eaf74e3ff8cde56399cf9297813f3833885064be70351eae286e6da9b",
        (2_u64, "partial", vec!["1000287502"]),
    );
    expected.insert(
        "h7-subject:round-exact-lender:91b2df274815b9997ac6ff6e772f2e142c7b3eac3ea61777131611896d6095a6",
        (172_u64, "none", vec!["1010297502"]),
    );

    for row in rows {
        let case_id = row["case_id"].as_str().expect("case id");
        let (residual_unit_bbls, after_reach, billing_bbls) =
            expected.get(case_id).expect("known remaining-five case");
        assert_eq!(
            required_u64(&row["residual_unit_bbls"]),
            *residual_unit_bbls
        );
        assert_eq!(
            row["after_pluto_vintage_reach"]
                .as_str()
                .expect("after reach"),
            *after_reach
        );
        assert_eq!(required_u64(&row["unit_bbls_with_geometry"]), 0);
        assert_eq!(
            required_u64(&row["billing_truth_members_after_pad_bridge"]),
            u64::try_from(billing_bbls.len()).expect("billing count fits")
        );
        assert_eq!(
            required_u64(&row["billing_truth_members_with_mappluto_geometry"]),
            u64::try_from(billing_bbls.len()).expect("billing count fits")
        );
        assert_string_array(&row["billing_bbls"], billing_bbls);
        assert_string_array(&row["billing_bbls_with_geometry"], billing_bbls);
    }
}

fn assert_condo_geometry_row_pins(artifact: &Value) {
    let rows = artifact["mappluto_billing_geometry_rows"]
        .as_array()
        .expect("billing geometry rows");
    assert_eq!(rows.len(), 12);

    let mut bbls = BTreeSet::new();
    let mut bbl_release_pairs = BTreeSet::new();
    let mut source_archives = BTreeSet::new();
    let mut transform_execution_ids = BTreeSet::new();
    for row in rows {
        let bbl = row["bbl"].as_str().expect("billing BBL");
        let release = row["release"].as_str().expect("release");
        assert!(matches!(release, "26v1" | "26v2"));
        bbls.insert(bbl.to_string());
        bbl_release_pairs.insert((bbl.to_string(), release.to_string()));
        assert_eq!(
            row["variant"].as_str().expect("variant"),
            "shoreline_clipped"
        );
        assert!(required_u64(&row["source_row_number"]) > 0);
        assert_eq!(
            row["source_filename"].as_str().expect("source filename"),
            "MapPLUTO.shp"
        );
        assert_sha256(row["source_archive_sha256"].as_str().expect("archive hash"));
        assert!(
            row["source_archive_s3_key"]
                .as_str()
                .expect("archive s3 key")
                .contains("/artifact=raw/")
        );
        assert_sha256(row["geom_wgs84_sha256"].as_str().expect("WGS84 hash"));
        assert_sha256(
            row["source_geom_wkb_sha256"]
                .as_str()
                .expect("source WKB hash"),
        );
        assert_eq!(
            row["geometry_evidence_contract_version"]
                .as_str()
                .expect("geometry evidence contract"),
            "nyc_dcp_mappluto_geometry_evidence.v3"
        );
        let transform_execution_id = row["transform_execution_id"]
            .as_str()
            .expect("transform execution id");
        assert!(transform_execution_id.starts_with("sha256-"));
        transform_execution_ids.insert(transform_execution_id.to_string());
        assert_eq!(
            row["transform_definition_id"]
                .as_str()
                .expect("transform definition id"),
            "sha256-ec1dc733d5f6e0ce38794baff7fa3d29d9d7369fd39b824550de7f8592bfac00"
        );
        assert_eq!(
            row["source_geometry_validity"]
                .as_str()
                .expect("source geometry validity"),
            "valid"
        );
        assert_eq!(row["geom_crs"].as_str().expect("geom CRS"), "EPSG:4326");
        assert_eq!(
            row["source_geom_crs"].as_str().expect("source geom CRS"),
            "EPSG:2263"
        );
        assert_eq!(
            row["source_crs_identifier"]
                .as_str()
                .expect("source CRS identifier"),
            "EPSG:2263"
        );
        assert_sha256(
            row["source_crs_wkt2_sha256"]
                .as_str()
                .expect("source CRS WKT hash"),
        );
        assert!(
            required_u64(&row["source_vertex_count"]) >= 5,
            "source polygon must have a nondegenerate exterior"
        );
        assert!(!row["is_current_release"].as_bool().expect("current flag"));
        source_archives.insert(
            row["source_archive_sha256"]
                .as_str()
                .expect("archive hash")
                .to_string(),
        );
    }

    assert_eq!(
        bbls,
        BTreeSet::from([
            "1000287502".to_string(),
            "1010297502".to_string(),
            "1012747504".to_string(),
            "1013267501".to_string(),
            "4050147502".to_string(),
            "4067977503".to_string(),
        ])
    );
    assert_eq!(bbl_release_pairs.len(), 12);
    assert_eq!(
        source_archives,
        BTreeSet::from([
            "84b213b86745c7daa4c75749ce7e9181633c834d70811280dbd9ea2045971876".to_string(),
            "e06eca9034731bc23f058bf532090e3c1ea6aed44a8128c6928f33872da34ab5".to_string(),
        ])
    );
    assert_eq!(
        transform_execution_ids,
        BTreeSet::from([
            "sha256-0416b2001f9c613b820c2a117840879f532b486379a99a9d4286bb41cf6f729f".to_string(),
            "sha256-139be5d84880248055ba44ea94e76f4c7968ab28ea2991cafd3a3463bc6ad538".to_string(),
        ])
    );
}

fn assert_condo_bridge_sql_boundary() {
    let sql = fs::read_to_string(CONDO_BILLING_GEOMETRY_SQL_PATH)
        .expect("read condo billing geometry SQL");
    assert!(sql.contains("SPLIT_PART(bbl, '.', 1)"));
    assert!(sql.contains("p.release = '26B'"));
    assert!(sql.contains("NYC_DCP_MAPPLUTO_GEOM_V3_EXT"));
    assert!(sql.contains("NYC_DCP_MAPPLUTO_GEOMETRY_EVIDENCE_EXT"));
    assert!(sql.contains("unit_bbls_with_mappluto_geom_v3"));
    assert!(sql.contains("billing_bbls_with_mappluto_geom_v3"));
    assert!(
        !sql.contains("ST_INTERSECTS"),
        "remaining-five bridge probe should classify row availability, not infer geometry truth"
    );
}

fn assert_condo_representation_population_artifact(
    artifact: &Value,
    base: &PopulationRequest,
    recovered: &PopulationRequest,
) {
    assert_eq!(recovered.cases.len(), base.cases.len());
    let rows = artifact["case_recovery_rows"]
        .as_array()
        .expect("case recovery rows");
    assert_eq!(rows.len(), 5);
    let rows_by_case: BTreeMap<String, &Value> = rows
        .iter()
        .map(|row| (row["case_id"].as_str().expect("case id").to_string(), row))
        .collect();
    let bridge = read_json(CONDO_BILLING_GEOMETRY_PATH);
    let bridge_by_case: BTreeMap<String, &Value> = bridge["case_bridge_rows"]
        .as_array()
        .expect("bridge rows")
        .iter()
        .map(|row| (row["case_id"].as_str().expect("case id").to_string(), row))
        .collect();

    let mut represented = BTreeSet::new();
    for (before, after) in base.cases.iter().zip(recovered.cases.iter()) {
        assert_eq!(after.id, before.id);
        assert_eq!(after.truth_plane, before.truth_plane);
        assert_eq!(after.truth.parcels, before.truth.parcels);

        let before_universe: BTreeSet<_> =
            before.evidence.universe.parcels.iter().cloned().collect();
        let after_universe: BTreeSet<_> = after.evidence.universe.parcels.iter().cloned().collect();
        let missing_before: BTreeSet<_> = before
            .truth
            .parcels
            .iter()
            .filter(|parcel| !before_universe.contains(*parcel))
            .cloned()
            .collect();

        if let Some(row) = rows_by_case.get(&before.id) {
            let bridge_row = bridge_by_case
                .get(&before.id)
                .expect("recovery row has bridge measurement");
            assert!(!missing_before.is_empty());
            assert_eq!(
                required_u64(&row["represented_unit_bbls_added_to_candidate_universe"]),
                u64::try_from(missing_before.len()).expect("missing count fits")
            );
            assert_eq!(
                row["first_represented_unit_bbl"]
                    .as_str()
                    .expect("first represented unit BBL"),
                missing_before.first().expect("first missing BBL").as_str()
            );
            assert_eq!(
                row["last_represented_unit_bbl"]
                    .as_str()
                    .expect("last represented unit BBL"),
                missing_before.last().expect("last missing BBL").as_str()
            );
            assert_eq!(
                row["after_pluto_vintage_reach"]
                    .as_str()
                    .expect("before representation reach"),
                bridge_row["after_pluto_vintage_reach"]
                    .as_str()
                    .expect("bridge reach")
            );
            assert_eq!(
                row["after_condo_representation_reach"]
                    .as_str()
                    .expect("after representation reach"),
                "full"
            );
            assert_eq!(
                required_u64(&row["unit_bbls_with_exact_mappluto_geometry"]),
                0
            );
            assert_eq!(
                string_array(&row["billing_bbls"]),
                string_array(&bridge_row["billing_bbls"])
            );
            assert_eq!(
                string_array(&row["billing_bbls_with_mappluto_geometry"]),
                string_array(&bridge_row["billing_bbls_with_geometry"])
            );
            assert_eq!(
                row["representation_choice"]
                    .as_str()
                    .expect("representation choice"),
                "condo_unit_bbl_identity_represented_by_pad_26b_billing_lot_geometry"
            );

            let mut expected_after = before_universe.clone();
            expected_after.extend(missing_before.iter().cloned());
            assert_eq!(
                after_universe, expected_after,
                "representation-widened universe drifted for {}",
                before.id
            );
            represented.extend(missing_before);
        } else {
            assert!(
                missing_before.is_empty(),
                "only the five bridge rows may retain missing truth before representation"
            );
            assert_eq!(
                after_universe, before_universe,
                "non-residual case changed in representation population: {}",
                before.id
            );
        }
    }

    assert_eq!(represented.len(), 331);
}

fn assert_condo_representation_bridge_replay(artifact: &Value) {
    let replay = &artifact["warehouse_replay"]["summary"];
    assert_eq!(required_u64(&replay["residual_cases"]), 5);
    assert_eq!(required_u64(&replay["residual_unit_bbls"]), 331);
    assert_eq!(required_u64(&replay["units_with_pad_row"]), 331);
    assert_eq!(required_u64(&replay["units_with_billing_bbl"]), 331);
    assert_eq!(required_u64(&replay["distinct_billing_bbls"]), 6);
    assert_eq!(required_u64(&replay["units_matched_by_exact_key"]), 6);
    assert_eq!(required_u64(&replay["units_matched_by_range"]), 325);
    assert_eq!(required_u64(&replay["unit_bbls_with_mappluto_geom_v3"]), 0);
    assert_eq!(
        required_u64(&replay["billing_bbls_with_mappluto_geom_v3"]),
        6
    );
    assert_eq!(required_u64(&replay["mappluto_geom_v3_billing_rows"]), 12);
    assert_eq!(
        required_u64(&replay["billing_rows_with_source_archive_sha256"]),
        12
    );
    assert_eq!(
        required_u64(&replay["billing_rows_with_geom_wgs84_sha256"]),
        12
    );
    assert_eq!(
        required_u64(&replay["billing_rows_with_source_geom_wkb_sha256"]),
        12
    );
    assert_eq!(
        required_u64(&replay["billing_rows_with_transform_execution_id"]),
        12
    );
    assert_string_array(
        &replay["billing_bbls"],
        &[
            "1000287502",
            "1010297502",
            "1012747504",
            "1013267501",
            "4050147502",
            "4067977503",
        ],
    );
    assert_string_array(&replay["billing_geometry_releases"], &["26v1", "26v2"]);
    assert_eq!(
        replay["pad_source_zip_sha256"]
            .as_str()
            .expect("PAD source hash"),
        "016a29968b4bed9e8dde10b9c27b68132aba994baf1dc3e2543a861eadfdf4bd"
    );
    assert_eq!(
        replay["pad_parser_version"].as_str().expect("PAD parser"),
        "2026-08-16"
    );

    let groups = artifact["unit_to_billing_groups"]
        .as_array()
        .expect("unit-to-billing groups");
    assert_eq!(groups.len(), 9);
    let represented: u64 = groups
        .iter()
        .map(|row| required_u64(&row["represented_unit_bbls"]))
        .sum();
    assert_eq!(represented, 331);
    let billing_bbls: BTreeSet<_> = groups
        .iter()
        .map(|row| {
            row["billing_bbl"]
                .as_str()
                .expect("billing BBL")
                .to_string()
        })
        .collect();
    assert_eq!(
        billing_bbls,
        BTreeSet::from([
            "1000287502".to_string(),
            "1010297502".to_string(),
            "1012747504".to_string(),
            "1013267501".to_string(),
            "4050147502".to_string(),
            "4067977503".to_string(),
        ])
    );
    let match_kinds: BTreeSet<_> = groups
        .iter()
        .map(|row| {
            row["pad_match_kind"]
                .as_str()
                .expect("PAD match kind")
                .to_string()
        })
        .collect();
    assert_eq!(
        match_kinds,
        BTreeSet::from(["exact_bbl_key".to_string(), "range_contains".to_string()])
    );

    let bridge = &artifact["representation_bridge"];
    assert_eq!(
        bridge["answer_grain"].as_str().expect("answer grain"),
        "billing_lot_representation_grain"
    );
    assert_eq!(
        bridge["identity_grain"].as_str().expect("identity grain"),
        "condo_unit_bbl"
    );
    assert_eq!(
        bridge["geometry_grain"].as_str().expect("geometry grain"),
        "pad_billing_bbl_mappluto_geometry"
    );
    assert_eq!(
        bridge["exact_unit_lot_geometry_status"]
            .as_str()
            .expect("exact unit geometry status"),
        "absent_from_landed_mappluto_geometry"
    );
    assert!(
        bridge["grain_caveat"]
            .as_str()
            .expect("bridge grain caveat")
            .contains("331 condo unit BBL identities collapse onto 6 billing-lot geometries")
    );
}

fn pluto_hit_bbls(artifact: &Value) -> BTreeSet<String> {
    artifact["pluto_vintage_hits"]
        .as_array()
        .expect("PLUTO vintage hits")
        .iter()
        .map(|row| row["bbl"].as_str().expect("hit BBL").to_string())
        .collect()
}

fn pluto_hits_are_historical_only(artifact: &Value) -> bool {
    artifact["pluto_vintage_hits"]
        .as_array()
        .expect("PLUTO vintage hits")
        .iter()
        .all(|row| required_u64(&row["current_release_rows"]) == 0)
}

fn assert_reach_counts(value: &Value, expected: &ReachCounts) {
    assert_eq!(required_u64(&value["full"]), expected.full);
    assert_eq!(required_u64(&value["partial"]), expected.partial);
    assert_eq!(required_u64(&value["none"]), expected.none);
}

fn assert_string_array(value: &Value, expected: &[&str]) {
    let actual = string_array(value);
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|entry| (*entry).to_string())
            .collect::<Vec<_>>()
    );
}

fn required_u64(value: &Value) -> u64 {
    value.as_u64().expect("nonnegative integer")
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|entry| entry.as_str().expect("string entry").to_string())
        .collect()
}

fn assert_sha256(value: &str) {
    assert_eq!(value.len(), 64);
    assert!(value.chars().all(|ch| ch.is_ascii_hexdigit()));
}

fn read_json(path: &str) -> Value {
    let file = File::open(path).expect("open JSON artifact");
    serde_json::from_reader(file).expect("parse JSON artifact")
}

fn read_population() -> PopulationRequest {
    read_population_from(POPULATION_PATH)
}

fn read_population_from(path: &str) -> PopulationRequest {
    let file = File::open(path).expect("open population request");
    let reader = BufReader::new(GzDecoder::new(file));
    serde_json::from_reader(reader).expect("parse population request")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(hex_digit(byte >> 4));
        hex.push(hex_digit(byte & 0x0f));
    }
    hex
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        10..=15 => char::from(b'a' + nibble - 10),
        _ => panic!("nibble out of range"),
    }
}
