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

const ARTIFACT_PATH: &str =
    "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/pluto_vintage_reach_measurement.json";
const POPULATION_PATH: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03/population_request_roll_universe.json.gz";
const SQL_PATH: &str = "scripts/geo_measurements/e4_pluto_vintage_reach.sql";
const WIDENED_POPULATION_PATH: &str =
    "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_widened.json.gz";

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
    assert!(pluto["content_hash_source"]
        .as_str()
        .expect("content hash source")
        .contains("NYC_DCP_PLUTO_MANIFEST_EXT"));
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
