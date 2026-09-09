use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

const REVIEW_PATH: &str = "scripts/geo_measurements/fixtures/e4_sec_property_row_bridge_2026-09-09/sec_property_row_bridge_review.json";

const TOTAL: &str = "total_unambiguous_exact_pad_row";
const AMBIGUOUS: &str = "ambiguous_multiple_pad_bbls_for_one_sec_property_row";
const UNBRIDGED: &str = "unbridged_no_exact_pad_address_row";
const OUT_OF_SCOPE: &str = "out_of_scope_non_nyc_property_row";

fn review() -> Value {
    let bytes = std::fs::read(REVIEW_PATH).expect("read SEC property row bridge review");
    serde_json::from_slice(&bytes).expect("review artifact is JSON")
}

fn as_array<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is an array"))
}

fn as_object<'a>(value: &'a Value, key: &str) -> &'a serde_json::Map<String, Value> {
    value[key]
        .as_object()
        .unwrap_or_else(|| panic!("{key} is an object"))
}

fn status_counts(rows: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in rows {
        let status = row["bridge_status"]
            .as_str()
            .expect("row bridge_status is a string");
        *counts.entry(status.to_string()).or_insert(0) += 1;
    }
    counts
}

fn case_by_short<'a>(cases: &'a [Value], short_id: &str) -> &'a Value {
    cases
        .iter()
        .find(|case| case["short_id"] == short_id)
        .unwrap_or_else(|| panic!("case {short_id} exists"))
}

fn count(case: &Value, status: &str) -> u64 {
    case["row_status_counts"][status]
        .as_u64()
        .unwrap_or_else(|| panic!("{status} count exists"))
}

#[test]
fn sec_property_row_bridge_review_keeps_denominator_and_boundary() {
    let review = review();

    assert_eq!(
        review["version"],
        "canon_geo_e4_sec_property_row_bridge_review.v0"
    );
    assert_eq!(review["bead"], "bd-3io8");
    assert_eq!(
        review["proof_class"],
        "retained_cmdrvl_data_warehouse_source_audit_not_live"
    );
    assert_eq!(review["release_claim_allowed"], false);
    assert_eq!(review["frozen_denominator"], 77);
    assert_eq!(review["reported_frozen_denominator"], 79);
    assert_eq!(review["retained_population_denominator"], 70);
    assert_eq!(review["retained_population_deficit"], 7);

    let boundary = as_object(&review, "boundary");
    assert_eq!(boundary["loan_property_count_inference_allowed"], false);
    assert_eq!(
        boundary["property_row_count_equality_safe_as_tax_lot_cardinality"],
        false
    );
    assert_eq!(boundary["admission_only"], true);
    assert!(
        boundary["statement"]
            .as_str()
            .expect("boundary statement")
            .contains("No rho.address.pad.membership widening")
    );

    let denominator = as_object(&review, "denominator");
    assert_eq!(denominator["subjects"], 18);
    assert_eq!(denominator["current_lip_rows"], 61);
    assert_eq!(denominator["current_ny_rows"], 48);
    assert_eq!(denominator["current_non_ny_or_null_rows"], 13);
    assert_eq!(denominator["retained_safe_pad_union_subject_count"], 6);
    assert_eq!(denominator["sec_tax_lot_all_of_ready_subject_count"], 0);

    assert_eq!(
        review["live_lip_probe"]["retained_build_rows_found_in_live_lip"],
        0
    );
    assert_eq!(
        review["live_lip_probe"]["current_lip_build_id"],
        "d012ffbb-f95f-4c29-9b3d-804f38ada29e"
    );
}

#[test]
fn sec_property_row_bridge_review_classifies_all_rows_and_subjects() {
    let review = review();
    let cases = as_array(&review, "cases");
    let rows = as_array(&review, "rows");

    assert_eq!(cases.len(), 18);
    assert_eq!(rows.len(), 61);
    assert_eq!(
        cases
            .iter()
            .map(|case| case["current_lip_rows"].as_u64().expect("rows"))
            .sum::<u64>(),
        61
    );

    let counts = status_counts(rows);
    assert_eq!(counts.get(TOTAL), Some(&16));
    assert_eq!(counts.get(AMBIGUOUS), Some(&1));
    assert_eq!(counts.get(UNBRIDGED), Some(&32));
    assert_eq!(counts.get(OUT_OF_SCOPE), Some(&12));

    let declared = &review["row_bridge_probe"]["rows_by_status"];
    assert_eq!(declared[TOTAL], 16);
    assert_eq!(declared[AMBIGUOUS], 1);
    assert_eq!(declared[UNBRIDGED], 32);
    assert_eq!(declared[OUT_OF_SCOPE], 12);

    let allowed = BTreeSet::from([
        TOTAL.to_string(),
        AMBIGUOUS.to_string(),
        UNBRIDGED.to_string(),
        OUT_OF_SCOPE.to_string(),
    ]);
    assert!(counts.keys().all(|status| allowed.contains(status)));

    let current_total_subjects: Vec<&str> = as_array(
        &review["row_bridge_probe"],
        "subjects_all_current_rows_total",
    )
    .iter()
    .map(|subject| subject.as_str().expect("subject id"))
    .collect();
    assert_eq!(current_total_subjects, vec!["13ff4751", "d2cfbf35"]);

    let thirteen = case_by_short(cases, "13ff4751");
    assert_eq!(count(thirteen, TOTAL), 7);
    assert_eq!(thirteen["safe_for_sec_tax_lot_all_of"], false);
    assert!(
        thirteen["refusal_reason"]
            .as_str()
            .expect("reason")
            .contains("current-build")
    );

    let d2cf = case_by_short(cases, "d2cfbf35");
    assert_eq!(count(d2cf, TOTAL), 3);
    assert_eq!(d2cf["safe_for_sec_tax_lot_all_of"], false);

    let f608 = case_by_short(cases, "f608ce46");
    assert_eq!(count(f608, TOTAL), 1);
    assert_eq!(count(f608, AMBIGUOUS), 1);

    let fifty_six = case_by_short(cases, "56f3e4fa");
    assert_eq!(fifty_six["current_non_ny_or_null_rows"], 3);
    assert_eq!(count(fifty_six, OUT_OF_SCOPE), 3);

    let f558 = case_by_short(cases, "f5588ba9");
    assert_eq!(f558["current_non_ny_or_null_rows"], 4);
    assert_eq!(count(f558, OUT_OF_SCOPE), 4);

    let retained_safe_but_not_row_bridge = ["20869bd8", "c1803335", "f5588ba9"];
    for short_id in retained_safe_but_not_row_bridge {
        let case = case_by_short(cases, short_id);
        assert_eq!(case["retained_subject_level_pad_union_safe"], true);
        assert_eq!(case["safe_for_sec_tax_lot_all_of"], false);
    }
}

#[test]
fn sec_property_row_bridge_pins_positive_pad_rows() {
    let review = review();
    let rows = as_array(&review, "rows");
    let pins = as_array(&review, "matched_pad_source_pins");
    assert_eq!(pins.len(), 18);

    let pin_rows: BTreeSet<u64> = pins
        .iter()
        .map(|pin| pin["source_row_number"].as_u64().expect("source row"))
        .collect();

    for pin in pins {
        assert_eq!(pin["release"], "26B");
        assert_eq!(pin["release_dt"], "2026-05-01");
        assert_eq!(pin["source_file"], "bobaadr.txt");
        assert_eq!(
            pin["source_zip_sha256"],
            "016a29968b4bed9e8dde10b9c27b68132aba994baf1dc3e2543a861eadfdf4bd"
        );
        assert_eq!(pin["parser_version"], "2026-08-16");
        assert!(
            pin["license_terms"]
                .as_str()
                .expect("license")
                .contains("NYC Department of City Planning")
        );
        assert!(
            pin["attribution_text"]
                .as_str()
                .expect("attribution")
                .contains("NYC Department of City Planning")
        );
    }

    for row in rows {
        let source_rows = row["pad_source_rows"]
            .as_array()
            .expect("pad source rows array");
        if source_rows.is_empty() {
            continue;
        }
        assert!(
            !row["parcel_ids"].as_array().expect("parcel ids").is_empty(),
            "matched PAD row carries parcel ids"
        );
        for source_row in source_rows {
            let source_row = source_row.as_u64().expect("source row number");
            assert!(
                pin_rows.contains(&source_row),
                "source row {source_row} has a retained pin"
            );
        }
    }
}

#[test]
fn sec_property_row_bridge_negative_no_count_only_handoff() {
    let review = review();
    let handoff = as_object(&review, "admission_overlay_handoff");
    let movement = as_object(&review["row_bridge_probe"], "expected_direct_e4_movement");

    assert_eq!(handoff["ready"], false);
    assert_eq!(
        handoff["all_of_exact_cardinality_relations"]
            .as_array()
            .expect("relations"),
        &Vec::<Value>::new()
    );
    assert!(
        handoff["reason"]
            .as_str()
            .expect("handoff reason")
            .contains("abstain and exclude no subsets")
    );

    assert_eq!(
        review["boundary"]["loan_property_count_inference_allowed"],
        false
    );
    assert_eq!(
        review["boundary"]["property_row_count_equality_safe_as_tax_lot_cardinality"],
        false
    );
    assert_eq!(
        review["row_bridge_probe"]["sec_tax_lot_all_of_ready"],
        false
    );
    assert!(
        review["row_bridge_probe"]["new_sec_tax_lot_all_of_subjects"]
            .as_array()
            .expect("ready subjects")
            .is_empty()
    );
    assert_eq!(movement["resolved"], 0);
    assert_eq!(movement["exactly_correct"], 0);
    assert_eq!(movement["completed_truth_exclusions"], 0);
    assert_eq!(movement["false_merges"], 0);

    let handoff_bead = review["acquisition_handoff"]["bead"]
        .as_str()
        .expect("handoff bead");
    assert_eq!(handoff_bead, "bd-taok");
    assert_eq!(review["acquisition_handoff"]["required"], true);
}
