#![forbid(unsafe_code)]

use canon::geo::assessment_roll::{
    GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID, GeoAssessmentRollOwnerExactNormalizationProfile,
    normalize_assessment_roll_owner_exact_key,
};
use canon::namekit::legal_suffix::LegalSuffixProfile;
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

const MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_owner_source_residuals_2026-09-08/measurement.json";
const POPULATION: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_condo_representation_widened.json.gz";
const OWNER_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_gsf_property_type_bands_2026-09-08/overlay_request_property_type_gsf_bands.json.gz";
const COMPLETENESS_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_collateral_completeness_2026-09-08/overlay_request_acris_document_legal_completeness.json.gz";
const ROLL_ROWS: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03/assessment_roll_fy2026p3_lots.json.gz";

#[test]
fn owner_source_residual_artifact_declares_zero_direct_movement() {
    let measurement = read_json(MEASUREMENT);
    assert_eq!(
        measurement["version"],
        "canon_geo_owner_source_residual_classification.v0"
    );
    assert_eq!(measurement["bead"], "bd-1u3x");
    assert_eq!(
        measurement["proof_class"],
        "retained_cmdrvl_data_warehouse_verification_not_live"
    );
    assert_eq!(measurement["release_claim_allowed"], false);
    assert_eq!(measurement["frozen_denominator"], 79);
    assert_eq!(measurement["retained_population_denominator"], 70);
    assert_eq!(measurement["baseline"]["truth_exclusions"], 8);
    assert_eq!(measurement["baseline"]["resolved"], 7);
    assert_eq!(measurement["baseline"]["exactly_correct"], 7);
    assert_eq!(measurement["baseline"]["false_merges"], 0);
    assert_eq!(
        measurement["measurement"]["recoverable_admission_lever"],
        "none"
    );
    assert_eq!(measurement["measurement"]["canon_binding_defect_count"], 0);
    assert_eq!(
        measurement["expected_direct_e4_movement"],
        json!({
            "truth_exclusions_delta": 0,
            "resolved_delta": 0,
            "exactly_correct_delta": 0,
            "false_merges_delta": 0,
            "conflict_delta": 0,
            "ambiguous_delta": 0
        })
    );

    let cases = cases_by_fragment(&measurement);
    assert_eq!(
        cases.keys().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from(["3899edce", "69dbf5da"])
    );
    assert_eq!(
        cases["3899edce"]["classification"],
        "source_owner_spelling_conflict"
    );
    assert_eq!(
        cases["69dbf5da"]["classification"],
        "current_owner_temporal_divergence_inside_complete_acris_truth"
    );
    for case in cases.values() {
        assert_eq!(case["evidence_present"], true);
        assert_eq!(case["canon_binding_defect"], false);
        assert_eq!(
            case["expected_direct_e4_movement"],
            measurement["expected_direct_e4_movement"]
        );
    }
}

#[test]
fn owner_source_residuals_match_retained_owner_observations() {
    let measurement = read_json(MEASUREMENT);
    let population = read_json_gz(POPULATION);
    let overlay = read_json_gz(OWNER_OVERLAY);

    for classified in measurement["cases"].as_array().expect("cases") {
        let fragment = classified["case_fragment"].as_str().expect("case fragment");
        let population_case = case_by_fragment(&population, fragment);
        let truth = string_set(&population_case["truth"]["parcels"]);
        assert_eq!(
            truth.len() as u64,
            classified["truth_lot_count"]
                .as_u64()
                .expect("truth lot count")
        );

        let overlay_case = overlay_case(&overlay, population_case["id"].as_str().expect("case id"));
        let observation = owner_observation(overlay_case);
        assert_eq!(observation["observation"]["kind"], "integer_sum_band");
        assert_eq!(
            observation["observation"]["min"],
            classified["owner_band"]["min"]
        );
        assert_eq!(
            observation["observation"]["max"],
            classified["owner_band"]["max"]
        );

        let values = observation["observation"]["values"]
            .as_array()
            .expect("owner values")
            .iter()
            .map(|value| {
                (
                    value["id"].as_str().expect("value id").to_string(),
                    value["value"].as_u64().expect("value"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let truth_sum = truth
            .iter()
            .map(|parcel| *values.get(parcel).expect("truth parcel has owner value"))
            .sum::<u64>();
        let failing = truth
            .iter()
            .filter(|parcel| values[*parcel] > 0)
            .cloned()
            .collect::<BTreeSet<_>>();

        assert_eq!(
            truth_sum,
            classified["truth_owner_sum"]
                .as_u64()
                .expect("truth owner sum")
        );
        assert_eq!(failing, string_set(&classified["failing_truth_lots"]));
        assert_eq!(
            (truth.len() - failing.len()) as u64,
            classified["passing_truth_owner_count"]
                .as_u64()
                .expect("passing truth owner count")
        );

        let source_records = source_record_ids(observation);
        let failing_evidence = &classified["failing_owner_evidence"];
        assert!(
            source_records.contains(
                failing_evidence["assessment_roll"]["source_record_id"]
                    .as_str()
                    .expect("roll source record id")
            )
        );
        assert!(
            source_records.contains(
                failing_evidence["acris_party"]["source_record_id"]
                    .as_str()
                    .expect("party source record id")
            )
        );
    }
}

#[test]
fn owner_source_residuals_keep_acris_completeness_conflict_visible() {
    let measurement = read_json(MEASUREMENT);
    let population = read_json_gz(POPULATION);
    let completeness = read_json_gz(COMPLETENESS_OVERLAY);

    for classified in measurement["cases"].as_array().expect("cases") {
        let fragment = classified["case_fragment"].as_str().expect("case fragment");
        let population_case = case_by_fragment(&population, fragment);
        let truth = string_set(&population_case["truth"]["parcels"]);
        let overlay_case = overlay_case(
            &completeness,
            population_case["id"].as_str().expect("case id"),
        );
        let all_of = completeness_observation(overlay_case, "all_of");
        let cardinality = completeness_observation(overlay_case, "exact_cardinality");
        let members = all_of["observation"]["members"]
            .as_array()
            .expect("all_of members")
            .iter()
            .map(|member| member["id"].as_str().expect("member id").to_string())
            .collect::<BTreeSet<_>>();

        assert_eq!(members, truth);
        assert_eq!(
            cardinality["observation"]["count"],
            classified["completeness"]["exact_cardinality"]
        );
        assert_eq!(
            all_of["source_records"]
                .as_array()
                .expect("source records")
                .len(),
            classified["truth_lot_count"]
                .as_u64()
                .expect("truth lot count") as usize
        );
        assert_eq!(classified["completeness"]["all_of_equals_truth"], true);
        assert_eq!(
            classified["completeness"]["proof_label"],
            "definitional_on_this_population_not_independent_precision"
        );
    }
}

#[test]
fn owner_source_residuals_validate_source_rows_without_unsafe_normalization() {
    let measurement = read_json(MEASUREMENT);
    let roll_rows = read_json_gz(ROLL_ROWS);
    let cases = cases_by_fragment(&measurement);
    let profile = GeoAssessmentRollOwnerExactNormalizationProfile::legal_suffix_numeric_ordinal(
        LegalSuffixProfile::RegabFirmIdentity,
    );

    let patchen = &cases["3899edce"]["failing_owner_evidence"];
    assert_eq!(patchen["bbl"], "3016950031");
    assert_eq!(roll_rows["3016950031"]["owner"], "316 PATCHEN LLC");
    assert_eq!(patchen["assessment_roll"]["owner"], "316 PATCHEN LLC");
    assert_eq!(patchen["acris_party"]["name"], "316 PARCHEN LLC");
    assert_eq!(patchen["acris_legal"]["street_name"], "PATCHEN AVENUE");
    assert_eq!(
        normalized_key(&patchen["assessment_roll"]["owner"], profile),
        "316 PATCHEN"
    );
    assert_eq!(
        normalized_key(&patchen["acris_party"]["name"], profile),
        "316 PARCHEN"
    );
    assert_ne!(
        normalized_key(&patchen["assessment_roll"]["owner"], profile),
        normalized_key(&patchen["acris_party"]["name"], profile),
        "PATCHEN/PARCHEN must not be normalized into an owner match"
    );
    let history_names = patchen["supporting_history"]
        .as_array()
        .expect("supporting history")
        .iter()
        .map(|row| row["name"].as_str().expect("history name"))
        .collect::<BTreeSet<_>>();
    assert_eq!(history_names, BTreeSet::from(["316 PATCHEN LLC"]));

    let patin = &cases["69dbf5da"]["failing_owner_evidence"];
    assert_eq!(patin["bbl"], "3087731066");
    assert_eq!(roll_rows["3087731066"]["owner"], "PATIN, MICHAEL");
    assert_eq!(roll_rows["3087731066"]["apt"], "PS7");
    assert_eq!(patin["assessment_roll"]["owner"], "PATIN, MICHAEL");
    assert_eq!(patin["assessment_roll"]["aptno"], "PS7");
    assert_eq!(
        patin["acris_party"]["name"],
        "1809 EMMONS AVENUE RETAIL LLC"
    );
    assert_eq!(patin["acris_legal"]["unit"], "PS7");
    assert_ne!(
        normalized_key(&patin["assessment_roll"]["owner"], profile),
        normalized_key(&patin["acris_party"]["name"], profile),
        "PATIN, MICHAEL is a different owner, not a suffix or ordinal variant"
    );
    assert_eq!(
        cases["69dbf5da"]["completeness"]["document_date"],
        "2017-11-14"
    );
    assert_eq!(patin["later_owner_history"]["document_date"], "2021-10-28");
    assert_eq!(patin["later_owner_history"]["name"], "PATIN, MICHAEL");
}

#[test]
fn owner_source_residual_artifact_pins_retained_inputs() {
    let measurement = read_json(MEASUREMENT);
    for (path_field, sha_field) in [
        ("population_path", "population_sha256"),
        ("owner_overlay_path", "owner_overlay_sha256"),
        ("completeness_overlay_path", "completeness_overlay_sha256"),
        ("assessment_roll_rows_path", "assessment_roll_rows_sha256"),
    ] {
        let relative = measurement["retained_inputs"][path_field]
            .as_str()
            .expect("retained input path");
        let expected = measurement["retained_inputs"][sha_field]
            .as_str()
            .expect("retained input sha256");
        assert_eq!(sha256_file(repo_path(relative)), expected);
    }
}

fn cases_by_fragment(measurement: &Value) -> BTreeMap<&str, &Value> {
    measurement["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| (case["case_fragment"].as_str().expect("case fragment"), case))
        .collect()
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

fn owner_observation(overlay_case: &Value) -> &Value {
    overlay_case["observations"]
        .as_array()
        .expect("observations")
        .iter()
        .find(|observation| {
            observation["contract_id"].as_str() == Some(GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID)
        })
        .expect("owner observation")
}

fn completeness_observation<'a>(overlay_case: &'a Value, kind: &str) -> &'a Value {
    overlay_case["observations"]
        .as_array()
        .expect("observations")
        .iter()
        .find(|observation| observation["observation"]["kind"].as_str() == Some(kind))
        .unwrap_or_else(|| panic!("{kind} completeness observation"))
}

fn source_record_ids(observation: &Value) -> BTreeSet<&str> {
    observation["source_records"]
        .as_array()
        .expect("source records")
        .iter()
        .map(|record| {
            record["source_record_id"]
                .as_str()
                .expect("source record id")
        })
        .collect()
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn normalized_key(
    value: &Value,
    profile: GeoAssessmentRollOwnerExactNormalizationProfile,
) -> String {
    normalize_assessment_roll_owner_exact_key(value.as_str().expect("owner string"), profile)
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
