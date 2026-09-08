#![forbid(unsafe_code)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

const MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_denominator_deficit_2026-09-08/measurement.json";
const H7_EXCLUDED: &str =
    "scripts/geo_measurements/fixtures/d1_residuals/h7_excluded_zero_candidate_subject.json";
const H7_POPULATION: &str =
    "scripts/geo_measurements/fixtures/d1_residuals/canon_geo_h7_population.v0.json";
const GATE_V2_REQUEST: &str = "tests/fixtures/geo/e4_gate_v2_population_request.json";
const GATE_V2_ENRICHMENT: &str = "tests/fixtures/geo/e4_gate_v2_evidence_enrichment.json";
const H4_EXTENSION: &str = "tests/fixtures/geo/e4_h4_extension.json";

#[test]
fn denominator_deficit_characterization_declares_scope_and_counts() {
    let measurement = read_json(MEASUREMENT);
    assert_eq!(
        measurement["version"],
        "canon_geo_e4_denominator_deficit_characterization.v0"
    );
    assert_eq!(measurement["bead"], "bd-1g4x");
    assert_eq!(measurement["owner"], "codex-geo-2");
    assert_eq!(
        measurement["proof_class"],
        "retained_fixture_and_plan_evidence_not_live"
    );
    assert_eq!(measurement["release_claim_allowed"], false);
    assert!(
        measurement["boundary"]
            .as_str()
            .expect("boundary")
            .contains("authorized denominator-ruling restatement")
    );
    assert_eq!(measurement["denominator"]["frozen_e4_subjects"], 77);
    assert_eq!(
        measurement["denominator"]["reported_frozen_e4_subjects"],
        79
    );
    assert_eq!(measurement["denominator"]["evaluated_subjects"], 70);
    assert_eq!(measurement["denominator"]["deficit_subjects"], 7);
    assert_eq!(measurement["denominator"]["reported_deficit_subjects"], 9);
    assert_eq!(measurement["denominator"]["classified_deficit_subjects"], 7);
    assert_eq!(
        measurement["denominator"]["classified_reported_deficit_subjects"],
        9
    );
    assert_eq!(
        measurement["denominator"]["unclassified_deficit_subjects"],
        0
    );
    assert_eq!(
        measurement["denominator"]["denominator_ruling"]["previous_frozen_denominator"],
        79
    );
    assert_eq!(
        measurement["denominator"]["denominator_ruling"]["restated_frozen_denominator"],
        77
    );

    let rows = rows_by_id(&measurement);
    assert_eq!(rows.len(), 9);
    assert_eq!(
        count_field(&rows, "classification"),
        BTreeMap::from([
            ("BLOCKED_ON_DATA_WE_DO_NOT_HAVE".to_string(), 6),
            ("BLOCKED_ON_IDENTITY_BRIDGE".to_string(), 1),
            ("EXCLUDED_BY_DENOMINATOR_RULING".to_string(), 2),
        ])
    );
    assert_eq!(
        count_field(&rows, "drop_stage"),
        BTreeMap::from([
            ("candidate_reach".to_string(), 1),
            ("truth_binding".to_string(), 3),
            ("universe_construction".to_string(), 5),
        ])
    );
    assert_eq!(
        measurement["summary"]["classification_counts"]["BLOCKED_ON_DATA_WE_DO_NOT_HAVE"],
        6
    );
    assert_eq!(
        measurement["summary"]["classification_counts"]["BLOCKED_ON_IDENTITY_BRIDGE"],
        1
    );
    assert_eq!(
        measurement["summary"]["classification_counts"]["EXCLUDED_BY_DENOMINATOR_RULING"],
        2
    );
    assert_eq!(
        measurement["summary"]["reported_classification_counts_before_ruling"]["OUT_OF_SCOPE"],
        3
    );
    assert_eq!(
        measurement["summary"]["max_denominator_movement_if_recovered"],
        7
    );
    assert_eq!(
        rows.values()
            .map(|row| row["max_denominator_movement_if_recovered"]
                .as_u64()
                .expect("movement"))
            .sum::<u64>(),
        7
    );
    assert_eq!(
        rows.values()
            .map(|row| row["immediate_exact_correct_movement"]
                .as_u64()
                .expect("immediate movement"))
            .sum::<u64>(),
        0
    );
}

#[test]
fn denominator_deficit_source_artifact_hashes_match_retained_inputs() {
    let measurement = read_json(MEASUREMENT);
    for source in measurement["source_artifacts"]
        .as_array()
        .expect("source artifacts")
    {
        let relative = source["path"].as_str().expect("source path");
        let expected = source["sha256"].as_str().expect("source sha256");
        assert_eq!(
            sha256_file(repo_path(relative)),
            expected,
            "{relative} hash changed without updating the denominator characterization"
        );
    }
}

#[test]
fn denominator_deficit_zero_candidate_subject_matches_exclusion_artifact() {
    let measurement = read_json(MEASUREMENT);
    let excluded = read_json(H7_EXCLUDED);
    let rows = rows_by_id(&measurement);
    let row = &rows["h7_zero_candidate_6ed280773520fa75508c912ef4cc1ddb"];
    let excluded_subjects = excluded["excluded_subjects"]
        .as_array()
        .expect("excluded subjects");
    assert_eq!(excluded_subjects.len(), 1);
    let subject = &excluded_subjects[0];

    assert_eq!(row["classification"], "BLOCKED_ON_DATA_WE_DO_NOT_HAVE");
    assert_eq!(row["drop_stage"], "candidate_reach");
    assert_eq!(row["follow_up_bead"], "bd-298d");
    assert_eq!(row["subject_id"], subject["subject_id"]);
    assert_eq!(row["loan_key"], subject["loan_key"]);
    assert_eq!(row["document_id"], subject["document_id"]);
    assert_eq!(row["truth_plane"], subject["truth_plane"]);
    assert_eq!(row["association_plane"], subject["association_plane"]);
    assert_eq!(
        string_set(&row["truth_parcels"]),
        string_set(&subject["truth_parcels"])
    );

    let release_rows = subject["release_rows"].as_array().expect("release rows");
    assert_eq!(release_rows.len(), 2);
    assert_eq!(
        release_rows
            .iter()
            .map(|release| release["candidate_parcel_count"]
                .as_u64()
                .expect("candidate count"))
            .sum::<u64>(),
        0
    );
    assert!(
        release_rows
            .iter()
            .all(|release| release["reach_status"] == "none")
    );
}

#[test]
fn denominator_deficit_historical_rows_are_duplicate_truth_sets() {
    let measurement = read_json(MEASUREMENT);
    let h7 = read_json(H7_POPULATION);
    let gate_v2 = read_json(GATE_V2_REQUEST);
    let enrichment = read_json(GATE_V2_ENRICHMENT);
    let h4 = read_json(H4_EXTENSION);
    let rows = rows_by_id(&measurement);
    let h7_by_truth = h7_subjects_by_truth(&h7);
    let gate_v2_by_id = gate_v2_cases_by_id(&gate_v2);
    let h4_by_id = h4_cases_by_id(&h4);

    let ced7 = &rows["gate_v2_duplicate_ced7ad9f0d74abf7"];
    assert_eq!(ced7["classification"], "BLOCKED_ON_IDENTITY_BRIDGE");
    assert_eq!(
        ced7["reported_classification_before_ruling"],
        "OUT_OF_SCOPE"
    );
    assert_eq!(ced7["drop_stage"], "truth_binding");
    assert_eq!(ced7["follow_up_bead"], Value::Null);
    assert_eq!(ced7["max_denominator_movement_if_recovered"], 1);
    let ced7_truth = string_set(&gate_v2_by_id["ced7ad9f0d74abf7"]["truth"]["parcels"]);
    let duplicate_truth = string_set(&gate_v2_by_id["3cf11e9a58e3b710"]["truth"]["parcels"]);
    assert_eq!(ced7_truth, duplicate_truth);
    assert_eq!(string_set(&ced7["truth_parcels"]), ced7_truth);
    assert_eq!(
        h7_by_truth[&truth_key(&ced7_truth)],
        ced7["duplicate_of"]["h7_subject_id"]
            .as_str()
            .expect("duplicate H7 subject")
    );
    let loans = enrichment_loans_by_case(&enrichment);
    assert_eq!(ced7["loan_key"], loans["ced7ad9f0d74abf7"]);
    assert_ne!(loans["ced7ad9f0d74abf7"], loans["3cf11e9a58e3b710"]);

    for (row_id, h4_case_id) in [
        (
            "h4_extension_duplicate_6eb465fd908bc59f",
            "6eb465fd908bc59f",
        ),
        (
            "h4_extension_duplicate_aacff3c254d36ae6",
            "aacff3c254d36ae6",
        ),
    ] {
        let row = &rows[row_id];
        assert_eq!(row["classification"], "EXCLUDED_BY_DENOMINATOR_RULING");
        assert_eq!(row["reported_classification_before_ruling"], "OUT_OF_SCOPE");
        assert_eq!(row["drop_stage"], "truth_binding");
        assert_eq!(row["follow_up_bead"], Value::Null);
        let truth = string_set(&h4_by_id[h4_case_id]["truth_parcels"]);
        assert_eq!(string_set(&row["truth_parcels"]), truth);
        assert_eq!(
            h7_by_truth[&truth_key(&truth)],
            row["duplicate_of"]["h7_subject_id"]
                .as_str()
                .expect("duplicate H7 subject")
        );
    }
}

#[test]
fn denominator_deficit_unmaterialized_slots_are_not_silent_subjects() {
    let measurement = read_json(MEASUREMENT);
    let rows = rows_by_id(&measurement);
    let unmaterialized = rows
        .iter()
        .filter(|(id, _)| id.starts_with("unmaterialized_nonduplicate_subject_slot_"))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(unmaterialized.len(), 5);

    for (id, row) in unmaterialized {
        assert_eq!(row["classification"], "BLOCKED_ON_DATA_WE_DO_NOT_HAVE");
        assert_eq!(row["drop_stage"], "universe_construction");
        assert_eq!(row["subject_id"], Value::Null, "{id}");
        assert_eq!(row["loan_key"], Value::Null, "{id}");
        assert_eq!(row["document_id"], Value::Null, "{id}");
        assert_eq!(row["case_id"], Value::Null, "{id}");
        assert!(row["truth_parcels"].as_array().expect("truth").is_empty());
        assert_eq!(row["follow_up_bead"], "bd-22t5");
        assert!(
            row["reason"]
                .as_str()
                .expect("reason")
                .contains("consensus-extension probe admitted zero")
        );
        assert_eq!(row["max_denominator_movement_if_recovered"], 1);
        assert_eq!(row["immediate_exact_correct_movement"], 0);
    }
}

fn rows_by_id(measurement: &Value) -> BTreeMap<String, Value> {
    measurement["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            (
                row["deficit_id"].as_str().expect("deficit id").to_string(),
                row.clone(),
            )
        })
        .collect()
}

fn count_field(rows: &BTreeMap<String, Value>, field: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in rows.values() {
        *counts
            .entry(row[field].as_str().expect("field value").to_string())
            .or_default() += 1;
    }
    counts
}

fn h7_subjects_by_truth(h7: &Value) -> BTreeMap<String, String> {
    let mut subjects = BTreeMap::new();
    for case in h7["population"]["cases"]
        .as_array()
        .expect("H7 population cases")
    {
        let truth = string_set(&case["truth"]["parcels"]);
        let subject_id = case["id"].as_str().expect("H7 subject id").to_string();
        subjects.entry(truth_key(&truth)).or_insert(subject_id);
    }
    subjects
}

fn gate_v2_cases_by_id(gate_v2: &Value) -> BTreeMap<String, Value> {
    gate_v2["cases"]
        .as_array()
        .expect("Gate V2 cases")
        .iter()
        .map(|case| {
            (
                case["id"].as_str().expect("Gate V2 case id").to_string(),
                case.clone(),
            )
        })
        .collect()
}

fn enrichment_loans_by_case(enrichment: &Value) -> BTreeMap<String, String> {
    enrichment["cases"]
        .as_array()
        .expect("enrichment cases")
        .iter()
        .map(|case| {
            (
                case["case_id"]
                    .as_str()
                    .expect("enrichment case id")
                    .to_string(),
                case["loan_key"].as_str().expect("loan key").to_string(),
            )
        })
        .collect()
}

fn h4_cases_by_id(h4: &Value) -> BTreeMap<String, Value> {
    h4["cases"]
        .as_array()
        .expect("H4 extension cases")
        .iter()
        .map(|case| {
            (
                case["case_id"].as_str().expect("H4 case id").to_string(),
                case.clone(),
            )
        })
        .collect()
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string item").to_string())
        .collect()
}

fn truth_key(truth: &BTreeSet<String>) -> String {
    truth.iter().cloned().collect::<Vec<_>>().join("|")
}

fn read_json(relative: &str) -> Value {
    let file =
        File::open(repo_path(relative)).unwrap_or_else(|error| panic!("open {relative}: {error}"));
    serde_json::from_reader(BufReader::new(file))
        .unwrap_or_else(|error| panic!("parse {relative}: {error}"))
}

fn sha256_file(path: impl AsRef<Path>) -> String {
    let mut file = File::open(path).expect("open file for sha256");
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer).expect("read file for sha256");
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    format!("{:x}", hasher.finalize())
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
