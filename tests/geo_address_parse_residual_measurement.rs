use serde_json::Value;

const SQL: &str =
    include_str!("../scripts/geo_measurements/address_parse_residual_pad26b_export.sql");
const RECEIPT: &str =
    include_str!("../scripts/geo_measurements/address_parse_residual_pad26b_export.receipt.json");
const DENOMINATOR_QUERY_ID: &str = "01c6c258-0821-aa0e-006c-c703088ec5da";
const BOUNDED_EXPORT_QUERY_ID: &str = "01c6c25d-0821-ab8c-006c-c703088f36ce";
const DISCARDED_FULL_EXPORT_QUERY_ID: &str = "01c6c25b-0821-ab8c-006c-c703088f34da";

fn receipt() -> Value {
    serde_json::from_str(RECEIPT).expect("receipt JSON must parse")
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
