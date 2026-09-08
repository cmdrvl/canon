#![forbid(unsafe_code)]

use canon::geo::{
    GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID, GeoRhoObservationKind,
    calibration_receipt_blake3,
};
use flate2::read::GzDecoder;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

const POPULATION: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_condo_representation_widened.json.gz";
const POLICY_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_rho_admission_policy_2026-09-08/overlay_request_roll_owner_threshold_mu_gsf.json.gz";
const PROPERTY_TYPE_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_gsf_property_type_bands_2026-09-08/overlay_request_property_type_gsf_bands.json.gz";
const MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_gsf_property_type_bands_2026-09-08/measurement.json";
const TARGET_GSF_EXCLUSION_CASES: [&str; 2] = [
    "h7-subject:non-round:1f301680cb0f2c88d43c40f0869779eca34a07dfaca12b657b4509486df60a46",
    "h7-subject:non-round:b76aa620c96bfba4de6cabde58b67c576c15ab7ad7e6a16d24e87a2069a30ec5",
];

#[test]
fn property_type_gsf_handoff_is_retained_and_ready_to_score() {
    let measurement = read_json(MEASUREMENT);
    assert_eq!(
        measurement["version"],
        "canon_geo_e4_gsf_property_type_band_handoff.v0"
    );
    assert_eq!(measurement["bead"], "bd-2nx4");
    assert_eq!(
        measurement["proof_class"],
        "retained_population_measurement_not_live"
    );
    assert_eq!(measurement["frozen_denominator"], 79);
    assert_eq!(measurement["retained_population_denominator"], 70);
    assert_eq!(measurement["score_handoff"]["population_path"], POPULATION);
    assert_eq!(
        measurement["score_handoff"]["source_policy_overlay_path"],
        POLICY_OVERLAY
    );
    assert_eq!(
        measurement["score_handoff"]["property_type_overlay_path"],
        PROPERTY_TYPE_OVERLAY
    );
    assert_eq!(
        measurement["score_handoff"]["full_70_score_status"],
        "READY_TO_SCORE_by_bd_1g4x_not_scored_by_bd_2nx4"
    );
    assert_eq!(
        measurement["baseline"]["reach_full_partial_none"],
        serde_json::json!([70, 0, 0])
    );
    assert_eq!(measurement["baseline"]["resolved"], serde_json::json!(6));
    assert_eq!(
        measurement["baseline"]["exactly_correct"],
        serde_json::json!(6)
    );
    assert_eq!(
        measurement["baseline"]["false_merges"],
        serde_json::json!(0)
    );
    assert_eq!(
        measurement["baseline"]["truth_exclusions"],
        serde_json::json!(17)
    );
    assert_eq!(
        measurement["score_handoff"]["overlay_sha256"],
        sha256_file(PROPERTY_TYPE_OVERLAY)
    );
    assert_eq!(
        calibration_receipt_blake3(&measurement["calibration_profile"])
            .expect("calibration profile hashes"),
        measurement["calibration_profile_blake3"]
            .as_str()
            .expect("calibration profile hash string")
    );
}

#[test]
fn property_type_gsf_overlay_changes_only_profiled_gsf_bands() {
    let measurement = read_json(MEASUREMENT);
    let policy_overlay = case_overlay_map(&read_json_gz::<Value>(POLICY_OVERLAY));
    let property_overlay = case_overlay_map(&read_json_gz::<Value>(PROPERTY_TYPE_OVERLAY));
    let expected_changed = changed_case_ids(&measurement);
    assert_eq!(policy_overlay.len(), 70);
    assert_eq!(property_overlay.len(), 70);
    assert_eq!(
        expected_changed.len(),
        measurement["score_handoff"]["changed_gsf_case_count"]
            .as_u64()
            .expect("changed count") as usize
    );

    let mut actual_changed = BTreeSet::new();
    for (case_id, policy_case) in &policy_overlay {
        let property_case = property_overlay
            .get(case_id)
            .expect("property overlay preserves case ids");
        if policy_case != property_case {
            actual_changed.insert(case_id.clone());
            assert_eq!(
                strip_profiled_gsf_delta(policy_case.clone()),
                strip_profiled_gsf_delta(property_case.clone()),
                "only GSF contract calibration fields and band min/max may change for {case_id}"
            );
            assert_eq!(
                gsf_value_member_ids(policy_case),
                gsf_value_member_ids(property_case),
                "property-type bands must not add candidates for {case_id}"
            );
            assert_eq!(
                gsf_source_record_ids(policy_case),
                gsf_source_record_ids(property_case),
                "property-type bands must preserve source records for {case_id}"
            );
        }
    }
    assert_eq!(actual_changed, expected_changed);
    assert!(
        actual_changed
            .iter()
            .any(|case_id| case_id.contains("1ad5bdba"))
    );
    assert!(
        actual_changed
            .iter()
            .any(|case_id| case_id.contains("1f301680"))
    );
    assert!(
        actual_changed
            .iter()
            .any(|case_id| case_id.contains("b76aa620"))
    );
}

#[test]
fn property_type_gsf_bands_keep_remaining_gsf_exclusion_truth_sets_inside_band() {
    let population = case_population_map(&read_json_gz::<Value>(POPULATION));
    let overlay = case_overlay_map(&read_json_gz::<Value>(PROPERTY_TYPE_OVERLAY));

    for case_id in TARGET_GSF_EXCLUSION_CASES {
        let truth = population
            .get(case_id)
            .expect("target population case")
            .get("truth")
            .and_then(|truth| truth.get("parcels"))
            .and_then(Value::as_array)
            .expect("truth parcels")
            .iter()
            .map(|value| value.as_str().expect("truth parcel string"))
            .collect::<BTreeSet<_>>();
        let observation = gsf_observation(overlay.get(case_id).expect("target overlay case"));
        let GeoRhoObservationKind::IntegerSumBand {
            values, min, max, ..
        } = serde_json::from_value(observation["observation"].clone())
            .expect("GSF observation parses")
        else {
            panic!("GSF observation must be an integer sum band");
        };
        let values_by_id = values
            .iter()
            .map(|value| (value.id.as_str(), value.value))
            .collect::<BTreeMap<_, _>>();
        let truth_sum = truth
            .iter()
            .map(|id| {
                values_by_id
                    .get(id)
                    .copied()
                    .unwrap_or_else(|| panic!("{case_id} truth value {id} must be present"))
            })
            .sum::<u64>();
        assert!(
            min <= truth_sum && truth_sum <= max,
            "{case_id} truth GSF {truth_sum} must fall inside property-type band {min}..{max}"
        );
    }
}

fn changed_case_ids(measurement: &Value) -> BTreeSet<String> {
    measurement["score_handoff"]["changed_gsf_cases"]
        .as_array()
        .expect("changed cases array")
        .iter()
        .map(|case| {
            case["case_id"]
                .as_str()
                .expect("changed case id")
                .to_string()
        })
        .collect()
}

fn case_overlay_map(overlay: &Value) -> BTreeMap<String, Value> {
    overlay["case_overlays"]
        .as_array()
        .expect("case overlays")
        .iter()
        .map(|case| {
            (
                case["case_id"].as_str().expect("case id").to_string(),
                case.clone(),
            )
        })
        .collect()
}

fn case_population_map(population: &Value) -> BTreeMap<String, Value> {
    population["cases"]
        .as_array()
        .expect("population cases")
        .iter()
        .map(|case| {
            (
                case["id"].as_str().expect("case id").to_string(),
                case.clone(),
            )
        })
        .collect()
}

fn strip_profiled_gsf_delta(mut case: Value) -> Value {
    for contract in case["contracts"]
        .as_array_mut()
        .expect("case contracts are an array")
    {
        if contract["id"] == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID {
            contract["method_version"] = serde_json::json!("profiled-gsf-band");
            contract["basis"]["population_id"] = serde_json::json!("profiled-gsf-population");
            contract["basis"]["calibration_blake3"] = serde_json::json!("profiled-gsf-calibration");
            contract["basis"]["falsification_rule_id"] =
                serde_json::json!("profiled-gsf-falsification-rule");
            contract["basis"]["admissible_hard_band"] = serde_json::json!(true);
        }
    }
    for observation in case["observations"]
        .as_array_mut()
        .expect("case observations are an array")
    {
        if observation["contract_id"] == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID {
            observation["observation"]["min"] = serde_json::json!(0);
            observation["observation"]["max"] = serde_json::json!(0);
        }
    }
    case
}

fn gsf_observation(case: &Value) -> &Value {
    case["observations"]
        .as_array()
        .expect("case observations")
        .iter()
        .find(|observation| {
            observation["contract_id"] == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        })
        .expect("GSF observation")
}

fn gsf_value_member_ids(case: &Value) -> Vec<String> {
    gsf_observation(case)["observation"]["values"]
        .as_array()
        .expect("GSF values")
        .iter()
        .map(|value| value["id"].as_str().expect("value id").to_string())
        .collect()
}

fn gsf_source_record_ids(case: &Value) -> Vec<String> {
    gsf_observation(case)["source_records"]
        .as_array()
        .expect("GSF source records")
        .iter()
        .map(|record| {
            record["source_record_id"]
                .as_str()
                .expect("source record id")
                .to_string()
        })
        .collect()
}

fn read_json_gz<T: DeserializeOwned>(relative: &str) -> T {
    let file = File::open(repo_path(relative)).expect("open gzipped fixture");
    serde_json::from_reader(GzDecoder::new(BufReader::new(file))).expect("parse gzipped fixture")
}

fn read_json(relative: &str) -> Value {
    let file = File::open(repo_path(relative)).expect("open JSON fixture");
    serde_json::from_reader(BufReader::new(file)).expect("parse JSON fixture")
}

fn sha256_file(relative: &str) -> String {
    let mut file = File::open(repo_path(relative)).expect("open fixture for sha256");
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

fn repo_path(relative: &str) -> impl AsRef<Path> {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
