#![forbid(unsafe_code)]

use canon::geo::{
    GeoPointPopulationArtifact, GeoPointPopulationErrorCode, GeoPointPopulationPoint,
    canonical_point_population_bytes, validate_point_population_artifact,
};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;

const GROSS_JSON: &str = include_str!("fixtures/geo/e1_gross_class_points.json");
const CONDO_JSON: &str = include_str!("fixtures/geo/e1_condo_points.json");
const GROSS_SQL: &[u8] = include_bytes!("../scripts/geo_measurements/e1_gross_class_points.sql");
const CONDO_SQL: &[u8] = include_bytes!("../scripts/geo_measurements/e1_condo_points.sql");

#[test]
fn t44_e1_point_population_fixtures_are_pinned_and_valid() {
    let gross = parse_artifact("gross", GROSS_JSON);
    let condo = parse_artifact("condo", CONDO_JSON);

    assert_population_fixture(
        "gross",
        &gross,
        "e1.gross_class.40",
        "fixture.e1.gross_class",
        40,
        GROSS_SQL,
    )
    .unwrap_or_else(|message| panic!("{message}"));
    assert_population_fixture(
        "condo",
        &condo,
        "e1.condo.31",
        "fixture.e1.condo",
        31,
        CONDO_SQL,
    )
    .unwrap_or_else(|message| panic!("{message}"));

    let billing_equals_pip = condo
        .points
        .iter()
        .filter(|point| point.billing_equals_pip == Some(true))
        .count();
    assert_eq!(
        billing_equals_pip, 10,
        "condo billing_equals_pip denominator changed: expected 10 actual {billing_equals_pip}"
    );

    let mut shortened = gross.clone();
    shortened.points.pop();
    let count_error = assert_population_fixture(
        "gross shortened",
        &shortened,
        "e1.gross_class.40",
        "fixture.e1.gross_class",
        40,
        GROSS_SQL,
    )
    .expect_err("removing one point must fail the declared denominator");
    assert!(
        count_error.contains("points.len() expected 40 actual 39"),
        "{count_error}"
    );

    let mut non_fixture = gross.clone();
    non_fixture.source_dataset = "nyc.pad.26b".to_string();
    let source_error = validate_point_population_artifact(&non_fixture)
        .expect_err("non-fixture source_dataset must be refused");
    assert_eq!(source_error.code, GeoPointPopulationErrorCode::InvalidInput);
    assert_eq!(
        source_error.detail.get("field").map(String::as_str),
        Some("source_dataset"),
        "{source_error:?}"
    );

    let first_point_id = gross.points[0].point_id.clone();
    let mut outside_nyc = gross;
    outside_nyc.points[0].landed_geocode.lon_e7 = -1_200_000_000;
    let bbox_error = validate_point_population_artifact(&outside_nyc)
        .expect_err("outside-NYC E7 coordinates must be refused");
    assert_eq!(bbox_error.code, GeoPointPopulationErrorCode::InvalidInput);
    assert_eq!(
        bbox_error.detail.get("field").map(String::as_str),
        Some("landed_geocode"),
        "{bbox_error:?}"
    );
    assert_eq!(
        bbox_error.detail.get("point_id").map(String::as_str),
        Some(first_point_id.as_str()),
        "{bbox_error:?}"
    );
}

#[test]
fn t45_point_population_canonical_bytes_are_key_order_deterministic() {
    for (label, source) in [("gross", GROSS_JSON), ("condo", CONDO_JSON)] {
        let artifact = parse_artifact(label, source);
        let first = canonical_point_population_bytes(&artifact)
            .unwrap_or_else(|error| panic!("{label}: canonical bytes failed: {error:?}"));
        let second = canonical_point_population_bytes(&artifact)
            .unwrap_or_else(|error| panic!("{label}: second canonical bytes failed: {error:?}"));
        assert_same_bytes(label, &first, &second);

        let value: Value =
            serde_json::from_str(source).unwrap_or_else(|error| panic!("{label}: JSON: {error}"));
        let reordered = reverse_object_keys(value);
        let reordered_artifact: GeoPointPopulationArtifact = serde_json::from_value(reordered)
            .unwrap_or_else(|error| panic!("{label}: reordered JSON parses: {error}"));
        let reordered_bytes = canonical_point_population_bytes(&reordered_artifact)
            .unwrap_or_else(|error| panic!("{label}: reordered canonical bytes failed: {error:?}"));
        assert_same_bytes(label, &first, &reordered_bytes);
    }
}

#[test]
fn t27_point_population_literals_stay_out_of_geo_engine_sources() {
    let gross = parse_artifact("gross", GROSS_JSON);
    let condo = parse_artifact("condo", CONDO_JSON);
    let forbidden = [
        gross.population_id.clone(),
        gross.source_dataset.clone(),
        gross.points[0].point_id.clone(),
        condo.population_id.clone(),
        condo.source_dataset.clone(),
        condo.points[0].point_id.clone(),
    ];

    for (path, source) in [
        ("src/geo/address.rs", include_str!("../src/geo/address.rs")),
        (
            "src/geo/composition.rs",
            include_str!("../src/geo/composition.rs"),
        ),
        ("src/geo/control.rs", include_str!("../src/geo/control.rs")),
        (
            "src/geo/discovery.rs",
            include_str!("../src/geo/discovery.rs"),
        ),
        (
            "src/geo/evaluation.rs",
            include_str!("../src/geo/evaluation.rs"),
        ),
        (
            "src/geo/evidence.rs",
            include_str!("../src/geo/evidence.rs"),
        ),
        (
            "src/geo/executor.rs",
            include_str!("../src/geo/executor.rs"),
        ),
        (
            "src/geo/geometry.rs",
            include_str!("../src/geo/geometry.rs"),
        ),
        (
            "src/geo/geometry_value.rs",
            include_str!("../src/geo/geometry_value.rs"),
        ),
        (
            "src/geo/identifiers.rs",
            include_str!("../src/geo/identifiers.rs"),
        ),
        (
            "src/geo/materialize.rs",
            include_str!("../src/geo/materialize.rs"),
        ),
        (
            "src/geo/multisource.rs",
            include_str!("../src/geo/multisource.rs"),
        ),
        ("src/geo/plan.rs", include_str!("../src/geo/plan.rs")),
        (
            "src/geo/residual_benchmark.rs",
            include_str!("../src/geo/residual_benchmark.rs"),
        ),
        ("src/geo/retry.rs", include_str!("../src/geo/retry.rs")),
        ("src/geo/run.rs", include_str!("../src/geo/run.rs")),
        ("src/geo/satisfy.rs", include_str!("../src/geo/satisfy.rs")),
        ("src/geo/stack.rs", include_str!("../src/geo/stack.rs")),
        ("src/geo/tile.rs", include_str!("../src/geo/tile.rs")),
    ] {
        let folded = source.to_ascii_lowercase();
        for literal in &forbidden {
            let literal = literal.to_ascii_lowercase();
            assert!(
                !folded.contains(&literal),
                "{path} must not hard-code point-population literal {literal}"
            );
        }
    }
}

fn parse_artifact(label: &str, source: &str) -> GeoPointPopulationArtifact {
    serde_json::from_str(source).unwrap_or_else(|error| panic!("{label}: fixture parses: {error}"))
}

fn assert_population_fixture(
    label: &str,
    artifact: &GeoPointPopulationArtifact,
    expected_population_id: &str,
    expected_source_dataset: &str,
    expected_count: usize,
    sql: &[u8],
) -> Result<(), String> {
    validate_point_population_artifact(artifact)
        .map_err(|error| format!("{label}: validator rejected fixture: {error:?}"))?;
    if artifact.population_id != expected_population_id {
        return Err(format!(
            "{label}: population_id expected {expected_population_id} actual {}",
            artifact.population_id
        ));
    }
    if artifact.source_dataset != expected_source_dataset {
        return Err(format!(
            "{label}: source_dataset expected {expected_source_dataset} actual {}",
            artifact.source_dataset
        ));
    }
    if !artifact.source_dataset.starts_with("fixture.") {
        return Err(format!(
            "{label}: source_dataset must start with fixture., actual {}",
            artifact.source_dataset
        ));
    }
    let actual_sql_sha256 = sha256_hex(sql);
    if artifact.selection_query_sha256 != actual_sql_sha256 {
        return Err(format!(
            "{label}: selection_query_sha256 expected {actual_sql_sha256} actual {}",
            artifact.selection_query_sha256
        ));
    }
    if artifact.points.len() != expected_count {
        return Err(format!(
            "{label}: points.len() expected {expected_count} actual {}",
            artifact.points.len()
        ));
    }
    assert_sorted_unique_points(label, &artifact.points)?;
    for point in &artifact.points {
        if point.home_cell_r9.is_empty() || point.loan_key.is_empty() {
            return Err(format!(
                "{label}: point {} has empty home_cell_r9 or loan_key",
                point.point_id
            ));
        }
    }
    Ok(())
}

fn assert_sorted_unique_points(
    label: &str,
    points: &[GeoPointPopulationPoint],
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for point in points {
        if let Some(previous_id) = previous
            && previous_id >= point.point_id.as_str()
        {
            return Err(format!(
                "{label}: first unsorted point_id pair previous={previous_id} current={}",
                point.point_id
            ));
        }
        if !seen.insert(point.point_id.as_str()) {
            return Err(format!("{label}: duplicate point_id {}", point.point_id));
        }
        previous = Some(point.point_id.as_str());
    }
    Ok(())
}

fn reverse_object_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.reverse();
            let mut reversed = Map::new();
            for (key, value) in entries {
                reversed.insert(key, reverse_object_keys(value));
            }
            Value::Object(reversed)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(reverse_object_keys).collect()),
        other => other,
    }
}

fn assert_same_bytes(label: &str, left: &[u8], right: &[u8]) {
    if left == right {
        return;
    }
    let first_diff = left
        .iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| left.len().min(right.len()));
    panic!(
        "{label}: canonical bytes differ left_blake3={} right_blake3={} first_diff_offset={} left_len={} right_len={}",
        blake3::hash(left).to_hex(),
        blake3::hash(right).to_hex(),
        first_diff,
        left.len(),
        right.len()
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
