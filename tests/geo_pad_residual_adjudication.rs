#![forbid(unsafe_code)]

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
    "scripts/geo_measurements/fixtures/e4_pad_residual_adjudication_2026-09-08/measurement.json";
const PAD_REVIEW: &str = "scripts/geo_measurements/fixtures/e4_pad_truth_binding_2026-09-08/pad_truth_binding_review.json";
const ACRIS_COMPLETENESS_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_collateral_completeness_2026-09-08/overlay_request_acris_document_legal_completeness.json.gz";

#[test]
fn pad_residual_adjudication_declares_no_direct_pad_lever() {
    let measurement = read_json(MEASUREMENT);
    assert_eq!(
        measurement["version"],
        "bd_2rwf_pad_residual_adjudication.v0"
    );
    assert_eq!(measurement["bead"], "bd-2rwf");
    assert_eq!(
        measurement["proof_class"],
        "retained_population_measurement_not_live"
    );
    assert!(
        !measurement["release_claim_allowed"]
            .as_bool()
            .expect("release claim flag")
    );
    assert_eq!(measurement["frozen_denominator"], 77);
    assert_eq!(measurement["reported_frozen_denominator"], 79);
    assert_eq!(measurement["retained_population_deficit"], 7);
    assert_eq!(measurement["retained_population_denominator"], 70);
    assert!(
        measurement["boundary"]
            .as_str()
            .expect("boundary")
            .contains("No PAD membership contract is widened")
    );

    assert_eq!(measurement["baseline"]["truth_exclusions"], 6);
    assert_eq!(measurement["baseline"]["truth_exclusions_completed"], 6);
    assert_eq!(measurement["baseline"]["reported_truth_exclusions"], 8);
    assert_eq!(
        measurement["baseline"]["truth_classification_incomplete"],
        2
    );
    assert_eq!(
        measurement["baseline"]["truth_classification_incomplete_case_fragments"],
        serde_json::json!(["91b2df27", "a7dac634"])
    );
    assert_eq!(measurement["baseline"]["resolved"], 7);
    assert_eq!(measurement["baseline"]["exactly_correct"], 7);
    assert_eq!(measurement["baseline"]["false_merges"], 0);
    assert_eq!(
        measurement["measurement"]["direct_pad_admission_lever"],
        "none"
    );
    assert!(
        !measurement["measurement"]["safe_canon_pad_lever_exists"]
            .as_bool()
            .expect("safe lever flag")
    );
    assert_eq!(
        measurement["measurement"]["bd_2rc0_fix_incomplete_cases"],
        0
    );
    assert_eq!(
        measurement["measurement"]["classification_counts"],
        json_object([("retained_truth_document_binding_conflict", 3)])
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

    let cases = cases_by_fragment(&measurement);
    assert_eq!(
        cases.keys().cloned().collect::<BTreeSet<_>>(),
        ["5934bb93", "a7dac634", "e3a5228d"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
}

#[test]
fn pad_residual_adjudication_reuses_reviewed_pad_truth_binding_facts() {
    let measurement = read_json(MEASUREMENT);
    let review = read_json(PAD_REVIEW);
    let cases = cases_by_fragment(&measurement);
    let review_cases = review_cases_by_short_id(&review);

    for (fragment, case) in &cases {
        let reviewed = review_cases.get(fragment.as_str()).expect("review case");
        assert_eq!(case["case_id"], reviewed["case_id"]);
        assert_eq!(
            case["filed_property"]["property_key"],
            reviewed["property_key"]
        );
        assert_eq!(
            case["filed_property"]["address"],
            reviewed["filed_property"]["address"]
        );
        assert_eq!(
            string_set(&case["pad"]["emitted_hard_members"]),
            string_set(&reviewed["emitted_pad_hard_members"])
        );
        assert_eq!(
            string_set(&case["pad"]["truth_bbls"]),
            string_set(&reviewed["truth_bbls"])
        );
        assert_eq!(
            case["filed_truth_relation"]["pad_emitted_address_family"],
            reviewed["filed_truth_relation"]["pad_emitted_address_family"]
        );
        assert_eq!(
            case["filed_truth_relation"]["pad_truth_address_family"],
            reviewed["filed_truth_relation"]["pad_truth_address_family"]
        );
        assert_eq!(case["filed_truth_relation"]["overlap"], "none");
        assert_eq!(reviewed["classification"], "different_property");
        assert!(
            case["pad"]["emitted_member_matches_filed_property"]
                .as_bool()
                .expect("emitted member matches filed property")
        );
        assert!(
            !case["settlement"]["bd_2rc0_fix_incomplete"]
                .as_bool()
                .expect("bd-2rc0 status")
        );
        assert_eq!(
            case["settlement"]["classification"],
            "retained_truth_document_binding_conflict"
        );
        assert_eq!(case["settlement"]["wrong_side"], "truth_source_binding");
    }

    assert_eq!(
        cases["e3a5228d"]["gsf_context"]["corrected_truth_sum_inside_band"],
        true
    );
}

#[test]
fn pad_residual_adjudication_explains_completeness_conflict_and_fallback_split() {
    let measurement = read_json(MEASUREMENT);
    let completeness = read_json_gz(ACRIS_COMPLETENESS_OVERLAY);
    let cases = cases_by_fragment(&measurement);

    for case in cases.values() {
        let overlay = overlay_case(&completeness, case["case_id"].as_str().expect("case id"));
        let all_of = overlay["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .find(|observation| observation["observation"]["kind"] == "all_of")
            .expect("all-of observation");
        let exact_cardinality = overlay["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .find(|observation| observation["observation"]["kind"] == "exact_cardinality")
            .expect("exact-cardinality observation");
        assert_eq!(
            member_ids(&all_of["observation"]["members"]),
            string_set(&case["pad"]["truth_bbls"])
        );
        assert_eq!(
            exact_cardinality["observation"]["count"]
                .as_u64()
                .expect("exact cardinality"),
            case["pad"]["truth_bbl_count"]
                .as_u64()
                .expect("truth count")
        );
    }

    assert!(
        cases["5934bb93"]["completeness_score_interaction"]["classified_as_completeness_conflict"]
            .as_bool()
            .expect("5934 conflict flag")
    );
    assert!(
        cases["e3a5228d"]["completeness_score_interaction"]["classified_as_completeness_conflict"]
            .as_bool()
            .expect("e3 conflict flag")
    );
    assert!(
        !cases["a7dac634"]["completeness_score_interaction"]["classified_as_completeness_conflict"]
            .as_bool()
            .expect("a7 conflict flag")
    );
    assert!(
        cases["a7dac634"]["completeness_score_interaction"]["reason"]
            .as_str()
            .expect("a7 reason")
            .contains("bd-3cez")
    );
    assert!(
        cases["a7dac634"]["completeness_score_interaction"]["reason"]
            .as_str()
            .expect("a7 reason")
            .contains("component_budget_fallback")
    );
}

#[test]
fn pad_residual_adjudication_pins_consumed_artifacts() {
    let measurement = read_json(MEASUREMENT);
    for (path_field, sha_field) in [
        (
            "pad_truth_binding_review_path",
            "pad_truth_binding_review_sha256",
        ),
        (
            "acris_completeness_overlay_path",
            "acris_completeness_overlay_sha256",
        ),
        ("population_path", "population_sha256"),
    ] {
        let relative = measurement["retained_inputs"][path_field]
            .as_str()
            .expect("retained input path");
        let expected = measurement["retained_inputs"][sha_field]
            .as_str()
            .expect("retained input sha256");
        assert_eq!(sha256_file(repo_path(relative)), expected);
    }

    assert_eq!(
        measurement["source_verification"]["consumed_review_measurement"],
        "bd_1hgq_pad_membership_binding_gaps_after_bd_2rc0"
    );
    assert!(
        measurement["source_verification"]["new_source_queries"]
            .as_array()
            .expect("new source queries")
            .is_empty()
    );
}

fn cases_by_fragment(measurement: &Value) -> BTreeMap<String, &Value> {
    measurement["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            (
                case["case_fragment"]
                    .as_str()
                    .expect("case fragment")
                    .to_string(),
                case,
            )
        })
        .collect()
}

fn review_cases_by_short_id(review: &Value) -> BTreeMap<String, &Value> {
    review["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            (
                case["short_id"].as_str().expect("short id").to_string(),
                case,
            )
        })
        .collect()
}

fn overlay_case<'a>(overlay: &'a Value, case_id: &str) -> &'a Value {
    overlay["case_overlays"]
        .as_array()
        .expect("case overlays")
        .iter()
        .find(|case| case["case_id"].as_str() == Some(case_id))
        .expect("case overlay")
}

fn member_ids(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("members")
        .iter()
        .map(|member| member["id"].as_str().expect("member id").to_string())
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
    let mut buffer = [0_u8; 8192];
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
