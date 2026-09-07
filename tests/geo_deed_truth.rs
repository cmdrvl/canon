#![forbid(unsafe_code)]

use canon::geo::{
    GeoDeedIndexRowsRequest, GeoDeedTruthArtifact, GeoDeedTruthLoanMatch, GeoDeedTruthLoanRef,
    GeoDeedTruthMatchKind, GeoDeedTruthProofClass, GeoPopulationErrorCode,
    GeoPopulationEvaluationRequest, GeoTruthPlane, canonical_deed_truth_bytes, derive_deed_truth,
    derive_deed_truth_from_index, evaluate_population, validate_deed_index_rows_request,
    validate_deed_truth_artifact, validate_deed_truth_plane_scope,
};

const DEED_INDEX_FIXTURE: &str = include_str!("fixtures/geo/deed_index_fixture.json");
const DEED_TRUTH_LOANS_FIXTURE: &str = include_str!("fixtures/geo/deed_truth_loans_fixture.json");
const FRANKLIN_POPULATION_FIXTURE: &str =
    include_str!("fixtures/geo/franklin_population_fixture.json");
const E4_POPULATION_REQUEST_FIXTURE: &str =
    include_str!("fixtures/geo/e4_gate_v2_population_request.json");
const FRANKLIN_DEED_TRUTH_EXPORT_SQL: &str =
    include_str!("../scripts/geo_measurements/e5_franklin_deed_truth_export.sql");

fn fixture_index() -> GeoDeedIndexRowsRequest {
    serde_json::from_str(DEED_INDEX_FIXTURE).expect("deed index fixture parses")
}

fn fixture_loans() -> Vec<GeoDeedTruthLoanRef> {
    serde_json::from_str(DEED_TRUTH_LOANS_FIXTURE).expect("deed truth loan fixture parses")
}

fn fixture_population() -> GeoPopulationEvaluationRequest {
    serde_json::from_str(FRANKLIN_POPULATION_FIXTURE).expect("Franklin population fixture parses")
}

fn derive_fixture_truth() -> GeoDeedTruthArtifact {
    let index = fixture_index();
    let loans = fixture_loans();
    validate_deed_index_rows_request(&index).expect("deed index validates");
    let artifact = derive_deed_truth_from_index(&loans, &index, 45).expect("deed truth derives");
    validate_deed_truth_artifact(&artifact).expect("deed truth validates");
    artifact
}

fn by_loan<'a>(artifact: &'a GeoDeedTruthArtifact, loan_id: &str) -> &'a GeoDeedTruthLoanMatch {
    artifact
        .per_loan
        .iter()
        .find(|row| row.loan_id == loan_id)
        .unwrap_or_else(|| panic!("loan {loan_id} is present in deed truth artifact"))
}

fn per_loan_debug(artifact: &GeoDeedTruthArtifact) -> String {
    artifact
        .per_loan
        .iter()
        .map(|row| {
            format!(
                "{}:{:?}:{:?}:{:?}:{:?}",
                row.loan_id,
                row.match_kind,
                row.matched_instrument_ids,
                row.parcel_ids,
                row.date_delta_days
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[test]
fn t48_deed_truth_discards_non_unique_and_reports_round_amount_stratum() {
    let artifact = derive_fixture_truth();
    let debug = per_loan_debug(&artifact);

    assert_eq!(artifact.version, "canon_geo_deed_truth.v0");
    assert_eq!(artifact.truth_plane, GeoTruthPlane::DeedGrainInstrument);
    assert_eq!(artifact.proof_class, GeoDeedTruthProofClass::Fixture);
    assert_eq!(artifact.window_days, 45);
    assert_eq!(artifact.summary.loans, 6, "{debug}");
    assert_eq!(artifact.summary.unique, 4, "{debug}");
    assert_eq!(artifact.summary.non_unique_discarded, 1, "{debug}");
    assert_eq!(artifact.summary.no_match, 1, "{debug}");
    assert_eq!(
        artifact.summary.unique + artifact.summary.non_unique_discarded + artifact.summary.no_match,
        artifact.summary.loans,
        "{debug}"
    );
    assert_eq!(artifact.summary.round_amount_loans, 1, "{debug}");
    assert_eq!(artifact.summary.round_amount_unique, 1, "{debug}");
    assert_eq!(artifact.summary.non_round_amount_loans, 5, "{debug}");
    assert_eq!(artifact.summary.non_round_amount_unique, 3, "{debug}");

    let unique_rows = artifact
        .per_loan
        .iter()
        .filter(|row| row.match_kind == GeoDeedTruthMatchKind::Unique)
        .collect::<Vec<_>>();
    assert_eq!(unique_rows.len(), 4, "{debug}");
    for row in unique_rows {
        assert!(
            !row.parcel_ids.is_empty(),
            "unique deed truth row must carry parcel ids: {debug}"
        );
        assert_eq!(
            row.matched_instrument_ids.len(),
            1,
            "unique deed truth row must name one instrument: {debug}"
        );
    }
    for row in artifact
        .per_loan
        .iter()
        .filter(|row| row.match_kind != GeoDeedTruthMatchKind::Unique)
    {
        assert!(
            row.parcel_ids.is_empty(),
            "non-truth deed row leaked parcel ids: {debug}"
        );
    }

    let non_unique = by_loan(&artifact, "loan-004");
    assert_eq!(
        non_unique.match_kind,
        GeoDeedTruthMatchKind::NonUniqueDiscarded
    );
    assert_eq!(
        non_unique.matched_instrument_ids,
        vec!["inst-004".to_string(), "inst-005".to_string()],
        "{debug}"
    );
    assert!(non_unique.parcel_ids.is_empty(), "{debug}");

    let empty_parcel_candidate = by_loan(&artifact, "loan-005");
    assert_eq!(
        empty_parcel_candidate.match_kind,
        GeoDeedTruthMatchKind::NoMatch
    );
    assert!(empty_parcel_candidate.amount_exact, "{debug}");
    assert_eq!(empty_parcel_candidate.date_delta_days, Some(9), "{debug}");
    assert!(
        empty_parcel_candidate.matched_instrument_ids.is_empty(),
        "empty-parcel candidate must not become truth: {debug}"
    );
    assert!(
        empty_parcel_candidate.parcel_ids.is_empty(),
        "empty-parcel candidate must not emit truth parcels: {debug}"
    );

    let round_unique = by_loan(&artifact, "loan-006");
    assert!(round_unique.round_amount, "{debug}");
    assert_eq!(round_unique.match_kind, GeoDeedTruthMatchKind::Unique);
    assert_eq!(round_unique.parcel_ids, vec!["parcel-006-a".to_string()]);

    let source_pin = artifact.source_pins.first().expect("artifact source pin");
    assert_eq!(
        source_pin.license_terms,
        "fixture-only-not-live-recorder-proof"
    );
    assert_eq!(
        source_pin.attribution_text,
        "fixture deed index for canon geo D7 tests"
    );
    assert_eq!(source_pin.source_content_sha256_field, "SOURCE_SHA256");
    assert_eq!(source_pin.source_release_field, "SOURCE_RELEASE");
    assert_eq!(source_pin.release_dt_field, "RELEASE_DT");
    assert_eq!(source_pin.parser_version_field, "PARSER_VERSION");

    let mut reversed_loans = fixture_loans();
    reversed_loans.reverse();
    let mut reversed_deeds = fixture_index().rows;
    reversed_deeds.reverse();
    let reordered = derive_deed_truth(&reversed_loans, &reversed_deeds, 45)
        .expect("reordered deed truth derives");
    assert_eq!(
        canonical_deed_truth_bytes(&artifact).expect("canonical bytes"),
        canonical_deed_truth_bytes(&reordered).expect("canonical bytes"),
        "deed truth derivation must be deterministic under input ordering"
    );
}

#[test]
fn t48_deed_truth_refuses_clear_address_fields() {
    let loans = fixture_loans();
    let mut index_with_address = fixture_index();
    index_with_address.rows[0].asserted_address = Some("1 Fixture Way".to_string());
    let deed_error = derive_deed_truth_from_index(&loans, &index_with_address, 45)
        .expect_err("deed row address must refuse");
    assert_eq!(
        deed_error.code,
        GeoPopulationErrorCode::DeedTruthAddressJoinDetected
    );
    assert_eq!(
        deed_error.detail.get("field").map(String::as_str),
        Some("deeds[].asserted_address")
    );

    let mut loans_with_address = fixture_loans();
    loans_with_address[0].asserted_address = Some("1 Fixture Way".to_string());
    let loan_error = derive_deed_truth_from_index(&loans_with_address, &fixture_index(), 45)
        .expect_err("loan address must refuse");
    assert_eq!(
        loan_error.code,
        GeoPopulationErrorCode::DeedTruthAddressJoinDetected
    );
    assert_eq!(
        loan_error.detail.get("field").map(String::as_str),
        Some("loans[].asserted_address")
    );
}

#[test]
fn t49_deed_truth_plane_is_not_pooled_with_other_truth_planes() {
    let artifact = derive_fixture_truth();
    let population = fixture_population();
    let unique_loan_ids = artifact
        .per_loan
        .iter()
        .filter(|row| row.match_kind == GeoDeedTruthMatchKind::Unique)
        .map(|row| row.loan_id.clone())
        .collect::<Vec<_>>();
    let scored_loan_ids = population
        .cases
        .iter()
        .map(|case| case.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(unique_loan_ids, scored_loan_ids);
    assert_eq!(
        artifact.summary.unique + artifact.summary.non_unique_discarded + artifact.summary.no_match,
        artifact.summary.loans
    );
    assert_eq!(population.cases.len() as u64, artifact.summary.unique);
    assert!(
        !scored_loan_ids.iter().any(|id| id == "loan-004"),
        "non-unique deed truth row must stay outside the scoring denominator"
    );
    assert!(
        !scored_loan_ids.iter().any(|id| id == "loan-005"),
        "no-match deed truth row must stay outside the scoring denominator"
    );

    let evaluation = evaluate_population(&population).expect("deed truth population evaluates");
    assert_eq!(evaluation.summary.cases, artifact.summary.unique);
    assert_eq!(evaluation.summary.truth_planes.len(), 1);
    let plane = &evaluation.summary.truth_planes[0];
    assert_eq!(plane.truth_plane, GeoTruthPlane::DeedGrainInstrument);
    assert_eq!(plane.cases, artifact.summary.unique);
    assert_eq!(plane.population_eligible_cases, artifact.summary.unique);
    assert_eq!(
        plane.candidate_reach_evaluated_cases,
        artifact.summary.unique
    );
    assert_eq!(plane.candidate_reach_full_cases, artifact.summary.unique);
    assert_eq!(plane.solver_truth_scored_cases, artifact.summary.unique);
    assert_eq!(plane.truth_members, 5);
    assert_eq!(plane.truth_members_in_universe, 5);
    assert!(
        evaluation
            .cases
            .iter()
            .all(|case| case.truth_plane == GeoTruthPlane::DeedGrainInstrument)
    );

    validate_deed_truth_plane_scope([GeoTruthPlane::DeedGrainInstrument])
        .expect("single deed plane is allowed");
    validate_deed_truth_plane_scope([
        GeoTruthPlane::GateV2Historical,
        GeoTruthPlane::AddressDerivedControl,
    ])
    .expect("existing non-deed mixed planes are outside the deed pooling guard");

    let direct_error = validate_deed_truth_plane_scope([
        GeoTruthPlane::DeedGrainInstrument,
        GeoTruthPlane::AddressDerivedControl,
    ])
    .expect_err("deed truth plane cannot be pooled");
    assert_eq!(
        direct_error.code,
        GeoPopulationErrorCode::DeedTruthPlanePooled
    );
    assert!(
        direct_error
            .detail
            .get("truth_planes")
            .is_some_and(|planes| planes.contains("DeedGrainInstrument")),
        "pooled-plane refusal must name the deed truth plane: {direct_error:?}"
    );

    let mut request: GeoPopulationEvaluationRequest =
        serde_json::from_str(E4_POPULATION_REQUEST_FIXTURE).expect("population fixture parses");
    request.cases.truncate(2);
    request.max_cases = 2;
    request.cases[0].truth_plane = GeoTruthPlane::DeedGrainInstrument;
    request.cases[1].truth_plane = GeoTruthPlane::AddressDerivedControl;
    let evaluation_error =
        evaluate_population(&request).expect_err("population evaluation must refuse pooling");
    assert_eq!(
        evaluation_error.code,
        GeoPopulationErrorCode::DeedTruthPlanePooled
    );
}

#[test]
fn e5_franklin_deed_truth_export_is_pinned_truth_plane_input() {
    for required in [
        "'canon_geo_deed_index_rows.v0'::TEXT AS output_contract",
        "'deed_grain_instrument'::TEXT AS truth_plane",
        "'fixture_class_not_scored'::TEXT AS absent_proof_class",
        "edgar_db.information_schema.tables",
        "edgar_db.information_schema.columns",
        "SOURCE_RELEASE",
        "RELEASE_DT",
        "SOURCE_SHA256",
        "PARSER_VERSION",
        "LICENSE_TERMS",
        "ATTRIBUTION_TEXT",
        "'truth_plane_only_not_candidate_evidence'",
        "'unique_plus_non_unique_discarded_plus_no_match_equals_loans'",
        "'mortgage_instrument_exact_amount_recording_window_unique_required'",
    ] {
        assert!(
            FRANKLIN_DEED_TRUTH_EXPORT_SQL.contains(required),
            "deed truth export SQL must contain {required:?}"
        );
    }

    let folded = FRANKLIN_DEED_TRUTH_EXPORT_SQL.to_ascii_lowercase();
    for forbidden in [
        "propertyaddress",
        "siteaddres",
        "situs_address",
        "street_address",
        "mailing_address",
        "geom_geog",
        "st_contains",
        "st_intersects",
        "limit 1",
        "result_scan",
    ] {
        assert!(
            !folded.contains(forbidden),
            "deed truth export SQL must not depend on {forbidden:?}"
        );
    }
}
