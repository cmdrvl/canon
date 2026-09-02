use serde_json::Value;
use std::collections::BTreeSet;

const SQL: &str =
    include_str!("../scripts/geo_measurements/address_parse_residual_pad26b_export.sql");
const RECEIPT: &str =
    include_str!("../scripts/geo_measurements/address_parse_residual_pad26b_export.receipt.json");
const CHARACTERIZATION_SQL: &str =
    include_str!("../scripts/geo_measurements/address_parse_residual_pad26b_characterization.sql");
const CHARACTERIZATION_RECEIPT: &str = include_str!(
    "../scripts/geo_measurements/address_parse_residual_pad26b_characterization.receipt.json"
);
const DENOMINATOR_QUERY_ID: &str = "01c6c258-0821-aa0e-006c-c703088ec5da";
const BOUNDED_EXPORT_QUERY_ID: &str = "01c6c25d-0821-ab8c-006c-c703088f36ce";
const DISCARDED_FULL_EXPORT_QUERY_ID: &str = "01c6c25b-0821-ab8c-006c-c703088f34da";
const CHARACTERIZATION_QUERY_ID: &str = "01c6ce22-0821-c675-006c-c703089a80c2";
const CHARACTERIZATION_EXAMPLES_QUERY_ID: &str = "01c6ce27-0821-c676-006c-c703089ab026";
const CHARACTERIZATION_SQL_SHA256: &str =
    "eecadbe0814cffe35b049f6abcffa660353e72d85317fee740feabe2a4267d07";

fn receipt() -> Value {
    serde_json::from_str(RECEIPT).expect("receipt JSON must parse")
}

fn characterization_receipt() -> Value {
    serde_json::from_str(CHARACTERIZATION_RECEIPT)
        .expect("characterization receipt JSON must parse")
}

fn query_receipt<'a>(value: &'a Value, purpose: &str) -> &'a Value {
    let matches = value["query_receipts"]
        .as_array()
        .expect("query receipts array")
        .iter()
        .filter(|receipt| receipt["purpose"] == purpose)
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "expected one receipt for {purpose}");
    matches[0]
}

fn is_live_positive_receipt(value: &Value) -> bool {
    let Some(bounded_export) = value["query_receipts"].as_array().and_then(|receipts| {
        receipts
            .iter()
            .find(|receipt| receipt["purpose"] == "pad26b_residual_bounded_export")
    }) else {
        return false;
    };

    value["proof_class"] == "live"
        && value["disposition"] == "bounded_positive_export"
        && value["query_as_of"] == "2026-08-31"
        && value["bounded_export"]["row_status"] == "ok"
        && value["bounded_export"]["observed_rows"]
            .as_i64()
            .unwrap_or(0)
            > 0
        && value["bounded_export"]["guard_rows"] == 0
        && value["bounded_export"]["observed_rows"]
            .as_i64()
            .unwrap_or(0)
            <= value["bounded_export"]["export_row_cap"]
                .as_i64()
                .unwrap_or(0)
        && value["denominators"]["pad_unresolved_keys"]
            .as_i64()
            .unwrap_or(0)
            > value["bounded_export"]["export_row_cap"]
                .as_i64()
                .unwrap_or(0)
        && bounded_export["status"] == "SUCCESS"
        && bounded_export["query_id"] == BOUNDED_EXPORT_QUERY_ID
        && bounded_export["rows_produced"] == value["bounded_export"]["observed_rows"]
        && bounded_export["total_elapsed_ms"].as_i64().unwrap_or(0) > 0
}

fn is_live_characterization_receipt(value: &Value) -> bool {
    let Some(classes) = value["classes"].as_array() else {
        return false;
    };
    let Some(characterization_query) = value["query_receipts"].as_array().and_then(|receipts| {
        receipts
            .iter()
            .find(|receipt| receipt["purpose"] == "pad26b_residual_characterization")
    }) else {
        return false;
    };
    let Some(examples_query) = value["query_receipts"].as_array().and_then(|receipts| {
        receipts
            .iter()
            .find(|receipt| receipt["purpose"] == "pad26b_residual_class_examples")
    }) else {
        return false;
    };

    let denominator = value["denominators"]["pad_unresolved_keys"]
        .as_i64()
        .unwrap_or(-1);
    let class_total = classes
        .iter()
        .map(|class| class["key_count"].as_i64().unwrap_or(-1))
        .sum::<i64>();
    let mut class_ids = BTreeSet::new();
    let classes_are_unique = classes.iter().all(|class| {
        let Some(id) = class["residual_class"].as_str() else {
            return false;
        };
        class_ids.insert(id)
    });
    let allowed_dispositions = [
        "fixable_here",
        "fixable_upstream",
        "structurally_unresolvable",
    ];
    let classes_are_well_formed = classes.iter().all(|class| {
        class["key_count"].as_i64().unwrap_or(0) > 0
            && class["example_keys"].as_array().is_some_and(|examples| {
                examples.len() >= 3 && examples.iter().all(|example| example.as_str().is_some())
            })
            && class["description"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
            && class["disposition"]
                .as_str()
                .is_some_and(|disposition| allowed_dispositions.contains(&disposition))
    });
    let canceled_queries_are_discarded =
        value["query_receipts"].as_array().is_some_and(|receipts| {
            receipts.iter().all(|receipt| {
                receipt["status"] != "CANCELED"
                    || receipt["disposition"] == "discarded_timeout_not_positive_evidence"
            })
        });

    value["proof_class"] == "live"
        && value["disposition"] == "characterized_residual"
        && value["query_as_of"] == "2026-09-02"
        && value["source_sql_sha256"] == CHARACTERIZATION_SQL_SHA256
        && value["source_pins"]["pad_address_table"]["release"] == "26B"
        && value["source_pins"]["pad_address_table"]["release_dt"] == "2026-05-01"
        && value["source_pins"]["geocode_table"]["asof_cutoff"] == "2026-08-01"
        && value["denominators"]["address_county_keys"].as_i64() == Some(5269)
        && value["denominators"]["pad_matched_keys"].as_i64() == Some(3930)
        && denominator == 1339
        && value["classification_integrity"]["row_status"] == "ok"
        && value["classification_integrity"]["class_rows"].as_u64() == Some(classes.len() as u64)
        && value["classification_integrity"]["distinct_class_rows"].as_u64()
            == Some(classes.len() as u64)
        && value["classification_integrity"]["classified_key_rows"].as_i64() == Some(denominator)
        && value["classification_integrity"]["distinct_classified_keys"].as_i64()
            == Some(denominator)
        && value["classification_integrity"]["unclassified_key_rows"].as_i64() == Some(0)
        && value["classification_integrity"]["overlap_key_count"].as_i64() == Some(0)
        && value["classification_integrity"]["classified_total"].as_i64() == Some(denominator)
        && value["classification_integrity"]["counts_sum_to_pad_unresolved_keys"].as_bool()
            == Some(true)
        && class_total == denominator
        && classes_are_unique
        && classes_are_well_formed
        && characterization_query["query_id"] == CHARACTERIZATION_QUERY_ID
        && characterization_query["status"] == "SUCCESS"
        && characterization_query["rows_produced"].as_u64() == Some(classes.len() as u64)
        && characterization_query["total_elapsed_ms"]
            .as_i64()
            .unwrap_or(0)
            > 0
        && examples_query["query_id"] == CHARACTERIZATION_EXAMPLES_QUERY_ID
        && examples_query["status"] == "SUCCESS"
        && examples_query["rows_produced"].as_i64() == Some(24)
        && canceled_queries_are_discarded
}

#[test]
fn sql_keeps_full_residual_denominator_separate_from_export_cap() {
    assert!(SQL.contains("1339 AS expected_pad_unresolved_keys"));
    assert!(SQL.contains("25 AS export_row_cap"));
    assert!(SQL.contains("t.pad_unresolved_keys = p.expected_pad_unresolved_keys"));
    assert!(!SQL.contains("t.pad_unresolved_keys <= p.export_row_cap"));
    assert!(!SQL.contains("t.pad_unresolved_keys <= p.row_cap"));
    assert!(SQL.contains("e.export_row_number <= p.export_row_cap"));
    assert!(SQL.contains("COUNT(*) OVER () AS bounded_result_rows"));
}

#[test]
fn sql_fail_closed_path_cannot_be_confused_with_positive_rows() {
    assert!(SQL.contains(concat!(
        "ok_rows AS (\n",
        "  SELECT\n",
        "    p.contract,\n",
        "    p.measurement_id,\n",
        "    'live' AS proof_class,\n",
        "    'ok' AS row_status"
    )));
    assert!(SQL.contains(concat!(
        "guard_failure_rows AS (\n",
        "  SELECT\n",
        "    p.contract,\n",
        "    p.measurement_id,\n",
        "    'not_positive_evidence' AS proof_class,\n",
        "    'guard_failure' AS row_status"
    )));
    assert!(SQL.contains("WHERE NOT g.guard_ok"));
    assert!(SQL.contains("CAST(0 AS NUMBER) AS bounded_result_rows"));
    assert!(SQL.contains("WHERE g.guard_ok"));
}

#[test]
fn sql_is_read_only_and_does_not_query_history_as_the_measurement() {
    let upper = SQL.to_ascii_uppercase();
    for forbidden in [
        "CREATE TABLE",
        "INSERT INTO",
        "MERGE INTO",
        "COPY INTO",
        "RESULT_SCAN",
        "QUERY_HISTORY",
    ] {
        assert!(
            !upper.contains(forbidden),
            "measurement SQL must not contain {forbidden}"
        );
    }
}

#[test]
fn characterization_sql_preserves_denominator_and_rejects_drift_or_overlap() {
    assert!(CHARACTERIZATION_SQL.contains("5269 AS expected_address_county_keys"));
    assert!(CHARACTERIZATION_SQL.contains("3930 AS expected_pad_matched_keys"));
    assert!(CHARACTERIZATION_SQL.contains("1339 AS expected_pad_unresolved_keys"));
    assert!(CHARACTERIZATION_SQL.contains("'26B' AS pad_release"));
    assert!(CHARACTERIZATION_SQL.contains("DATE '2026-05-01' AS pad_release_dt"));
    assert!(CHARACTERIZATION_SQL.contains("DATE '2026-08-01' AS geocode_asof_cutoff"));
    assert!(CHARACTERIZATION_SQL.contains("FROM ks\n  WHERE PAD_BBLS = 0"));
    assert!(CHARACTERIZATION_SQL.contains("i.classified_total = p.expected_pad_unresolved_keys"));
    assert!(CHARACTERIZATION_SQL.contains("i.class_rows = i.distinct_class_rows"));
    assert!(CHARACTERIZATION_SQL.contains("k.classified_key_rows = k.distinct_classified_keys"));
    assert!(CHARACTERIZATION_SQL.contains("k.unclassified_key_rows = 0"));
    assert!(CHARACTERIZATION_SQL.contains("'classification_overlapping_key'"));
    assert!(CHARACTERIZATION_SQL.contains("'classification_not_exhaustive'"));
    assert!(CHARACTERIZATION_SQL.contains("'denominator_drift:pad_unresolved_keys'"));
    assert!(CHARACTERIZATION_SQL.contains("WHERE g.guard_ok"));
    assert!(CHARACTERIZATION_SQL.contains("WHERE NOT g.guard_ok"));

    for class_id in [
        "placeholder_or_non_street_delivery_form",
        "alias_or_multi_address_string",
        "queens_hyphenate_unmatched",
        "compound_or_range_house_number",
        "missing_structured_street",
        "missing_or_unparsed_house_number",
        "pad_street_present_number_absent",
        "street_not_seen_in_pad_borough",
    ] {
        assert!(
            CHARACTERIZATION_SQL.contains(class_id),
            "missing residual class {class_id}"
        );
    }

    let upper = CHARACTERIZATION_SQL.to_ascii_uppercase();
    for forbidden in [
        "CREATE TABLE",
        "INSERT INTO",
        "MERGE INTO",
        "COPY INTO",
        "RESULT_SCAN",
        "QUERY_HISTORY",
    ] {
        assert!(
            !upper.contains(forbidden),
            "characterization SQL must not contain {forbidden}"
        );
    }
}

#[test]
fn live_receipt_requires_nonzero_rows_and_does_not_promote_fixture_or_retained() {
    let receipt = receipt();
    assert!(is_live_positive_receipt(&receipt));

    let mut fixture = receipt.clone();
    fixture["proof_class"] = Value::String("fixture".to_string());
    assert!(!is_live_positive_receipt(&fixture));

    let mut retained = receipt.clone();
    retained["proof_class"] = Value::String("retained".to_string());
    assert!(!is_live_positive_receipt(&retained));

    let mut empty = receipt.clone();
    empty["bounded_export"]["observed_rows"] = Value::from(0);
    assert!(!is_live_positive_receipt(&empty));

    let mut guard_failure = receipt.clone();
    guard_failure["bounded_export"]["row_status"] = Value::String("guard_failure".to_string());
    guard_failure["bounded_export"]["guard_rows"] = Value::from(1);
    assert!(!is_live_positive_receipt(&guard_failure));
}

#[test]
fn live_receipt_binds_the_exact_completed_export_query() {
    let receipt = receipt();
    let denominator = query_receipt(&receipt, "pad26b_residual_denominator_control");
    assert_eq!(denominator["query_id"], DENOMINATOR_QUERY_ID);
    assert_eq!(denominator["status"], "SUCCESS");

    let export = query_receipt(&receipt, "pad26b_residual_bounded_export");
    assert_eq!(export["query_id"], BOUNDED_EXPORT_QUERY_ID);
    assert_eq!(export["status"], "SUCCESS");
    assert_eq!(
        export["rows_produced"],
        receipt["bounded_export"]["observed_rows"]
    );
    assert_eq!(export["rows_produced"], 25);

    let discarded = query_receipt(&receipt, "discarded_full_residual_export_attempt");
    assert_eq!(discarded["query_id"], DISCARDED_FULL_EXPORT_QUERY_ID);
    assert_eq!(discarded["status"], "CANCELED");
    assert_eq!(
        discarded["disposition"],
        "discarded_timeout_not_positive_evidence"
    );

    let mut wrong_query = receipt.clone();
    query_receipt_mut(&mut wrong_query, "pad26b_residual_bounded_export")["query_id"] =
        Value::String(DENOMINATOR_QUERY_ID.to_string());
    assert!(!is_live_positive_receipt(&wrong_query));

    let mut canceled_query = receipt.clone();
    query_receipt_mut(&mut canceled_query, "pad26b_residual_bounded_export")["query_id"] =
        Value::String(DISCARDED_FULL_EXPORT_QUERY_ID.to_string());
    assert!(!is_live_positive_receipt(&canceled_query));
}

fn query_receipt_mut<'a>(value: &'a mut Value, purpose: &str) -> &'a mut Value {
    let matches = value["query_receipts"]
        .as_array_mut()
        .expect("query receipts array")
        .iter_mut()
        .enumerate()
        .filter_map(|(index, receipt)| (receipt["purpose"] == purpose).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "expected one receipt for {purpose}");
    &mut value["query_receipts"][matches[0]]
}

#[test]
fn receipt_preserves_pins_denominators_and_discarded_timeout_boundary() {
    let receipt = receipt();
    assert_eq!(receipt["query_as_of"], "2026-08-31");
    assert_eq!(
        receipt["source_pins"]["geocode_table"]["table"],
        "EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED"
    );
    assert_eq!(
        receipt["source_pins"]["pad_address_table"]["table"],
        "EDGAR_DB.SOURCE.NYC_DCP_PAD_ADDRESS_HOT"
    );
    assert_eq!(
        receipt["source_pins"]["pad_address_table"]["release"],
        "26B"
    );
    assert_eq!(
        receipt["source_pins"]["pad_address_table"]["release_dt"],
        "2026-05-01"
    );
    assert_eq!(
        receipt["source_pins"]["geocode_table"]["asof_cutoff"],
        "2026-08-01"
    );
    assert_eq!(receipt["denominators"]["address_county_keys"], 5269);
    assert_eq!(receipt["denominators"]["pad_matched_keys"], 3930);
    assert_eq!(receipt["denominators"]["pad_unresolved_keys"], 1339);
    assert_eq!(receipt["denominators"]["pad_unique_keys"], 2337);
    assert_eq!(receipt["denominators"]["pad_multi_bbl_keys"], 1593);
    assert_eq!(receipt["denominators"]["pad_bbl_edges"], 6469);
    assert_eq!(receipt["denominators"]["max_pad_bbls_per_key"], 92);
    assert_eq!(
        receipt["source_pins"]["pad_address_table"]["source_zip_sha256"],
        "016a29968b4bed9e8dde10b9c27b68132aba994baf1dc3e2543a861eadfdf4bd"
    );
    assert_eq!(receipt["bounded_export"]["observed_rows"], 25);
    assert_eq!(receipt["bounded_export"]["export_row_cap"], 25);
    assert!(
        receipt["denominators"]["pad_unresolved_keys"]
            .as_i64()
            .expect("pad unresolved denominator")
            > receipt["bounded_export"]["export_row_cap"]
                .as_i64()
                .expect("export cap")
    );
    assert!(
        receipt["claim_boundary"]
            .as_str()
            .expect("claim boundary")
            .contains("not the full 1339-row corpus")
    );
}

#[test]
fn receipt_commits_no_raw_property_addresses_or_result_rows() {
    let receipt = receipt();
    for forbidden_top_level in ["rows", "result_rows", "sample_rows", "results"] {
        assert!(
            receipt.get(forbidden_top_level).is_none(),
            "receipt must not commit {forbidden_top_level}"
        );
    }
    assert!(
        !contains_key_ignore_case(&receipt, "property_address"),
        "receipt must not commit raw property-address fields"
    );
    assert!(
        !RECEIPT.to_ascii_uppercase().contains("QUERY_HISTORY"),
        "receipt must not cite a QUERY_HISTORY self-row as the export"
    );
}

#[test]
fn characterization_receipt_is_disjoint_exhaustive_and_example_backed() {
    let receipt = characterization_receipt();
    assert!(is_live_characterization_receipt(&receipt));

    let expected: [(&str, &str, i64, [&str; 3]); 8] = [
        (
            "placeholder_or_non_street_delivery_form",
            "structurally_unresolvable",
            7,
            [
                "Brooklyn Package|36047",
                "Various|36047",
                "C/o Gumley Haft|36061",
            ],
        ),
        (
            "alias_or_multi_address_string",
            "fixable_here",
            59,
            [
                "2136/2138 Honeywell Avenue a/k/a 901/903 E 181st St|36005",
                "2542A, 2544A, 2546A, 2548A WHITE PLAINS ROAD|36005",
                "2542A, 2544A, 2546A, 2548A White Plains Road|36005",
            ],
        ),
        (
            "queens_hyphenate_unmatched",
            "fixable_here",
            196,
            [
                "10-30 Beach 19th Street|36081",
                "10-36/10-48 Neilson Street|36081",
                "100-11 67th Road|36081",
            ],
        ),
        (
            "compound_or_range_house_number",
            "fixable_here",
            113,
            [
                "1264-1270 GERARD AVENUE|36005",
                "1441-1443 and|36005",
                "1964-1966 Newbold Avenue|36005",
            ],
        ),
        (
            "missing_structured_street",
            "fixable_here",
            11,
            [
                "3231|36005",
                "Creston & Prospect|36005",
                "Stockton & Nostrand|36047",
            ],
        ),
        (
            "missing_or_unparsed_house_number",
            "fixable_here",
            9,
            [
                "Dekalb & Kosciuszko|36047",
                "FOREST HILL INN APARTMENTS, INC.|36047",
                "Grove & Menahan|36047",
            ],
        ),
        (
            "pad_street_present_number_absent",
            "fixable_upstream",
            609,
            [
                "1100 Grand Concourse|36005",
                "1197 East 233rd Street|36005",
                "1299 GRAND CONCOURSE|36005",
            ],
        ),
        (
            "street_not_seen_in_pad_borough",
            "fixable_here",
            335,
            [
                "1054 Grant Avenue|36005",
                "106 Mount Hope Place|36005",
                "1261 Seabury Avenue|36005",
            ],
        ),
    ];

    let classes = receipt["classes"].as_array().expect("classes array");
    assert_eq!(classes.len(), expected.len());
    let mut sum = 0_i64;
    for (index, (class_id, disposition, count, examples)) in expected.iter().enumerate() {
        let class = &classes[index];
        assert_eq!(class["residual_class"], *class_id);
        assert_eq!(class["disposition"], *disposition);
        assert_eq!(class["key_count"], Value::from(*count));
        assert_eq!(
            class["class_order"],
            Value::from(i64::try_from(index + 1).expect("small class index"))
        );
        let observed_examples = class["example_keys"]
            .as_array()
            .expect("example keys")
            .iter()
            .map(|value| value.as_str().expect("example key"))
            .collect::<Vec<_>>();
        assert_eq!(observed_examples.as_slice(), examples);
        sum += *count;
    }
    assert_eq!(
        sum,
        receipt["denominators"]["pad_unresolved_keys"]
            .as_i64()
            .expect("unresolved denominator")
    );
    assert_eq!(sum, 1339);

    let characterization = query_receipt(&receipt, "pad26b_residual_characterization");
    assert_eq!(characterization["query_id"], CHARACTERIZATION_QUERY_ID);
    assert_eq!(characterization["rows_produced"], 8);
    let examples = query_receipt(&receipt, "pad26b_residual_class_examples");
    assert_eq!(examples["query_id"], CHARACTERIZATION_EXAMPLES_QUERY_ID);
    assert_eq!(examples["rows_produced"], 24);
}

#[test]
fn characterization_receipt_rejects_drift_canceled_queries_and_overlaps() {
    let receipt = characterization_receipt();
    assert!(is_live_characterization_receipt(&receipt));

    let mut different_release = receipt.clone();
    different_release["source_pins"]["pad_address_table"]["release"] =
        Value::String("26C".to_string());
    assert!(!is_live_characterization_receipt(&different_release));

    let mut narrowed_denominator = receipt.clone();
    narrowed_denominator["denominators"]["pad_unresolved_keys"] = Value::from(1338);
    assert!(!is_live_characterization_receipt(&narrowed_denominator));

    let mut duplicate_class = receipt.clone();
    let duplicate_id = duplicate_class["classes"][0]["residual_class"].clone();
    duplicate_class["classes"][1]["residual_class"] = duplicate_id;
    assert!(!is_live_characterization_receipt(&duplicate_class));

    let mut overlapping_key = receipt.clone();
    overlapping_key["classification_integrity"]["overlap_key_count"] = Value::from(1);
    overlapping_key["classification_integrity"]["distinct_classified_keys"] = Value::from(1338);
    assert!(!is_live_characterization_receipt(&overlapping_key));

    let mut unclassified = receipt.clone();
    unclassified["classification_integrity"]["unclassified_key_rows"] = Value::from(1);
    assert!(!is_live_characterization_receipt(&unclassified));

    let mut count_drift = receipt.clone();
    count_drift["classes"][0]["key_count"] = Value::from(8);
    assert!(!is_live_characterization_receipt(&count_drift));

    let mut canceled_primary = receipt.clone();
    query_receipt_mut(&mut canceled_primary, "pad26b_residual_characterization")["status"] =
        Value::String("CANCELED".to_string());
    assert!(!is_live_characterization_receipt(&canceled_primary));

    let mut canceled_not_discarded = receipt;
    query_receipt_mut(
        &mut canceled_not_discarded,
        "discarded_full_residual_export_attempt_from_bd158y",
    )["disposition"] = Value::String("bounded_positive_export".to_string());
    assert!(!is_live_characterization_receipt(&canceled_not_discarded));
}

fn contains_key_ignore_case(value: &Value, needle: &str) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case(needle) || contains_key_ignore_case(value, needle)
        }),
        Value::Array(values) => values
            .iter()
            .any(|value| contains_key_ignore_case(value, needle)),
        _ => false,
    }
}
