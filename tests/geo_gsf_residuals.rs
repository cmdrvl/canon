#![forbid(unsafe_code)]

use canon::geo::GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID;
use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

const MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_gsf_residuals_2026-09-08/measurement.json";
const POPULATION: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_condo_representation_widened.json.gz";
const OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_gsf_value_vector_rebuild_2026-09-08/overlay_request_current_universe_gsf_vectors.json.gz";
const ROLL_ROWS: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03/assessment_roll_fy2026p3_lots.json.gz";

#[test]
fn gsf_residual_classification_declares_no_direct_gsf_lever() {
    let measurement = read_json(MEASUREMENT);
    assert_eq!(
        measurement["version"],
        "canon_geo_gsf_residual_classification.v0"
    );
    assert_eq!(measurement["bead"], "bd-22eo");
    assert_eq!(
        measurement["proof_class"],
        "retained_population_measurement_not_live"
    );
    assert_eq!(measurement["release_claim_allowed"], false);
    assert_eq!(measurement["frozen_denominator"], 79);
    assert_eq!(measurement["baseline"]["truth_exclusions"], 8);
    assert_eq!(measurement["baseline"]["resolved"], 7);
    assert_eq!(measurement["baseline"]["exactly_correct"], 7);
    assert_eq!(measurement["baseline"]["false_merges"], 0);
    assert_eq!(
        measurement["measurement"]["direct_gsf_admission_lever"],
        "none"
    );
    assert_eq!(
        measurement["expected_direct_e4_movement"],
        json_object([
            ("ambiguous_delta", 0),
            ("exactly_correct_delta", 0),
            ("false_merges_delta", 0),
            ("resolved_delta", 0),
            ("truth_exclusions_delta", 0),
        ])
    );

    let case_fragments = measurement["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| case["case_fragment"].as_str().expect("case fragment"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        case_fragments,
        BTreeSet::from(["68a1a4ce", "91b2df27", "e1ee0535"])
    );
    for case in measurement["cases"].as_array().expect("cases") {
        assert_eq!(case["classification"], "source_truth_binding_issue");
        assert_eq!(case["evidence_present"], true);
        assert_eq!(case["corrected_truth_sum_inside_band"], false);
    }
}

#[test]
fn gsf_residual_cases_use_current_vectors_and_still_fail_band() {
    let measurement = read_json(MEASUREMENT);
    let population = read_json_gz(POPULATION);
    let overlay = read_json_gz(OVERLAY);

    for classified in measurement["cases"].as_array().expect("cases") {
        let fragment = classified["case_fragment"].as_str().expect("case fragment");
        let case = case_by_fragment(&population, fragment);
        let overlay_case = overlay_case(&overlay, case["id"].as_str().expect("case id"));
        let observation = gsf_observation(overlay_case);
        let values = observation["observation"]["values"]
            .as_array()
            .expect("integer values");
        let values_by_id = values
            .iter()
            .map(|value| {
                (
                    value["id"].as_str().expect("value id"),
                    value["value"].as_u64().expect("value"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let value_ids = values_by_id.keys().copied().collect::<BTreeSet<_>>();
        let universe = case["evidence"]["universe"]["parcels"]
            .as_array()
            .expect("universe parcels")
            .iter()
            .map(|parcel| parcel.as_str().expect("parcel id"))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            value_ids, universe,
            "{fragment} must use the current bound universe"
        );
        assert_eq!(
            values.len() as u64,
            classified["vector_count"].as_u64().expect("vector count")
        );

        let truth_sum = case["truth"]["parcels"]
            .as_array()
            .expect("truth parcels")
            .iter()
            .map(|parcel| {
                let parcel = parcel.as_str().expect("truth parcel");
                values_by_id
                    .get(parcel)
                    .copied()
                    .unwrap_or_else(|| panic!("{fragment} truth parcel {parcel} lacks GSF value"))
            })
            .sum::<u64>();
        let min = observation["observation"]["min"].as_u64().expect("min");
        let max = observation["observation"]["max"].as_u64().expect("max");
        assert_eq!(
            truth_sum,
            classified["corrected_truth_gsf_sum"]
                .as_u64()
                .expect("truth sum")
        );
        assert_eq!(min, classified["band_min"].as_u64().expect("band min"));
        assert_eq!(max, classified["band_max"].as_u64().expect("band max"));
        assert!(
            truth_sum < min || truth_sum > max,
            "{fragment} is a residual only if the current vector still fails the band"
        );

        let source_records = observation["source_records"]
            .as_array()
            .expect("source records")
            .iter()
            .map(|record| {
                record["source_record_id"]
                    .as_str()
                    .expect("source record id")
            })
            .collect::<BTreeSet<_>>();
        for parcel in case["truth"]["parcels"].as_array().expect("truth parcels") {
            let expected = format!(
                "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:gsf:{}",
                parcel.as_str().expect("truth parcel")
            );
            assert!(
                source_records.contains(expected.as_str()),
                "{fragment} truth parcel must carry a retained source-record pin"
            );
        }
    }
}

#[test]
fn gsf_residual_classification_is_binding_not_missing_values() {
    let measurement = read_json(MEASUREMENT);
    let population = read_json_gz(POPULATION);
    let roll_rows = read_json_gz(ROLL_ROWS);

    assert_eq!(
        measurement["measurement"]["truth_bbls_with_source_rows"],
        177
    );
    assert_eq!(
        measurement["measurement"]["truth_bbls_with_non_null_gsf"],
        177
    );

    let summaries = measurement["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| (case["case_fragment"].as_str().expect("case fragment"), case))
        .collect::<BTreeMap<_, _>>();

    assert_truth_roll_summary(&population, &roll_rows, summaries["68a1a4ce"]);
    assert_truth_roll_summary(&population, &roll_rows, summaries["91b2df27"]);
    assert_truth_roll_summary(&population, &roll_rows, summaries["e1ee0535"]);

    assert_eq!(
        summaries["68a1a4ce"]["filed_property"]["address"],
        "208 East 14th Street"
    );
    assert_eq!(
        summaries["91b2df27"]["filed_property"]["address"],
        "5 East 22nd Street Units Garage, Commercial A and Commercial B"
    );
    assert_eq!(
        summaries["e1ee0535"]["filed_property"]["address"],
        "450-460 PARK AVENUE SOUTH"
    );
    assert_eq!(
        summaries["91b2df27"]["truth_roll_summary"]["representative_street"],
        "217 WEST 57TH STREET"
    );
    assert_eq!(
        summaries["e1ee0535"]["truth_roll_summary"]["truth_unit_count"],
        542
    );
}

#[test]
fn gsf_residual_scope_excludes_cases_that_no_longer_fail_gsf() {
    let measurement = read_json(MEASUREMENT);
    let population = read_json_gz(POPULATION);
    let overlay = read_json_gz(OVERLAY);
    let excluded = measurement["excluded_from_scope"]
        .as_array()
        .expect("excluded scope")
        .iter()
        .map(|case| {
            (
                case["case_fragment"].as_str().expect("case fragment"),
                case["reason"].as_str().expect("reason"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert!(
        excluded["e3a5228d"].contains("inside the admitted MU band"),
        "e3a5228d must remain outside this GSF residual bead"
    );
    assert!(
        excluded["524efa30"].contains("inside band"),
        "524efa30 is a composed PAD+GSF case, not a remaining GSF residual"
    );

    for fragment in ["e3a5228d", "524efa30"] {
        let case = case_by_fragment(&population, fragment);
        let overlay_case = overlay_case(&overlay, case["id"].as_str().expect("case id"));
        let observation = gsf_observation(overlay_case);
        let values = observation["observation"]["values"]
            .as_array()
            .expect("values")
            .iter()
            .map(|value| {
                (
                    value["id"].as_str().expect("value id"),
                    value["value"].as_u64().expect("value"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let truth_sum = case["truth"]["parcels"]
            .as_array()
            .expect("truth parcels")
            .iter()
            .map(|parcel| values[parcel.as_str().expect("truth parcel")])
            .sum::<u64>();
        let min = observation["observation"]["min"].as_u64().expect("min");
        let max = observation["observation"]["max"].as_u64().expect("max");
        assert!(
            min <= truth_sum && truth_sum <= max,
            "{fragment} corrected GSF vector should not be classified as a GSF residual"
        );
    }
}

fn assert_truth_roll_summary(population: &Value, roll_rows: &Value, classified: &Value) {
    let fragment = classified["case_fragment"].as_str().expect("case fragment");
    let case = case_by_fragment(population, fragment);
    let truth = case["truth"]["parcels"].as_array().expect("truth parcels");
    assert_eq!(
        truth.len() as u64,
        classified["truth_roll_summary"]["truth_parcel_count"]
            .as_u64()
            .expect("truth parcel count")
    );

    let mut class_counts = BTreeMap::<String, u64>::new();
    let mut owners = BTreeSet::<String>::new();
    let mut units = 0;
    let mut with_gsf = 0;
    for parcel in truth {
        let parcel = parcel.as_str().expect("truth parcel");
        let row = &roll_rows[parcel];
        let class = row["cls"].as_str().expect("roll class").to_string();
        *class_counts.entry(class).or_default() += 1;
        owners.insert(row["owner"].as_str().expect("owner").to_string());
        units += parse_u64(&row["units"]);
        if parse_u64(&row["gross_sqft"]) > 0 {
            with_gsf += 1;
        }
    }
    assert_eq!(with_gsf, truth.len());
    assert_eq!(
        owners.len() as u64,
        classified["truth_roll_summary"]["distinct_owner_count"]
            .as_u64()
            .expect("owner count")
    );
    assert_eq!(
        units,
        classified["truth_roll_summary"]["truth_unit_count"]
            .as_u64()
            .expect("unit count")
    );
    let expected_classes = classified["truth_roll_summary"]["class_counts"]
        .as_object()
        .expect("class counts")
        .iter()
        .map(|(class, count)| (class.clone(), count.as_u64().expect("class count")))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(class_counts, expected_classes);
}

fn gsf_observation(overlay_case: &Value) -> &Value {
    overlay_case["observations"]
        .as_array()
        .expect("observations")
        .iter()
        .find(|observation| {
            observation["contract_id"].as_str()
                == Some(GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID)
        })
        .expect("GSF observation")
}

fn case_by_fragment<'a>(population: &'a Value, fragment: &str) -> &'a Value {
    let matches = population["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .filter(|case| case["id"].as_str().is_some_and(|id| id.contains(fragment)))
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "{fragment} must identify one population case"
    );
    matches[0]
}

fn overlay_case<'a>(overlay: &'a Value, case_id: &str) -> &'a Value {
    overlay["case_overlays"]
        .as_array()
        .expect("overlays")
        .iter()
        .find(|case| case["case_id"].as_str() == Some(case_id))
        .expect("overlay case")
}

fn parse_u64(value: &Value) -> u64 {
    value
        .as_str()
        .expect("numeric string")
        .parse()
        .expect("unsigned integer")
}

fn json_object<const N: usize>(pairs: [(&str, i64); N]) -> Value {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), Value::from(value)))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn read_json(relative: &str) -> Value {
    let file = File::open(repo_path(relative)).expect("open JSON fixture");
    serde_json::from_reader(BufReader::new(file)).expect("parse JSON fixture")
}

fn read_json_gz(relative: &str) -> Value {
    let file = File::open(repo_path(relative)).expect("open gzipped fixture");
    serde_json::from_reader(GzDecoder::new(BufReader::new(file))).expect("parse gzipped fixture")
}

fn sha256_file(path: impl AsRef<Path>) -> String {
    let mut file = File::open(path).expect("open fixture for sha256");
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer).expect("read fixture for sha256");
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

#[test]
fn gsf_residual_artifact_pins_retained_inputs() {
    let measurement = read_json(MEASUREMENT);
    for (path_field, sha_field) in [
        ("population_path", "population_sha256"),
        ("overlay_path", "overlay_sha256"),
        (
            "gsf_value_vector_rebuild_measurement_path",
            "gsf_value_vector_rebuild_measurement_sha256",
        ),
    ] {
        let relative = measurement["retained_inputs"][path_field]
            .as_str()
            .expect("path");
        let expected = measurement["retained_inputs"][sha_field]
            .as_str()
            .expect("sha256");
        assert_eq!(sha256_file(repo_path(relative)), expected);
    }
}
