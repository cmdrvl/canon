#![forbid(unsafe_code)]

use canon::entity::{
    edge::{EdgeEvidenceHit, build_edge_evidence_record},
    evidence::{
        CategoricalMatchSupportRequest, DateExactSupportRequest, DateNearSupportRequest,
        NumericWithinToleranceSupportRequest, categorical_match_support_hit,
        date_exact_support_hit, date_near_support_hit, numeric_within_tolerance_support_hit,
        validate_numeric_tolerance_params,
    },
    score::{ScoreLane, ScoreUnits},
};

#[test]
fn structured_support_numeric_boundaries_are_deterministic() {
    let absolute_hit = numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
        namespace: "neutral.structured",
        operator_id: "numeric_within_tolerance:reported_amount",
        reason_code: "amount_within_absolute_tolerance",
        view_name: "reported_amount",
        left_value: "100.00",
        right_value: "100.04",
        absolute_tolerance: "0.05",
        relative_tolerance_bps: None,
        score_units: score(8_000),
    })
    .expect("valid fixed decimals compare")
    .expect("absolute tolerance supports");
    assert_eq!(absolute_hit.lane, ScoreLane::Support);
    assert_eq!(absolute_hit.score_units, score(8_000));

    let absolute_boundary =
        numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
            namespace: "neutral.structured",
            operator_id: "numeric_within_tolerance:reported_amount",
            reason_code: "amount_within_absolute_tolerance",
            view_name: "reported_amount",
            left_value: "100.00",
            right_value: "100.05",
            absolute_tolerance: "0.05",
            relative_tolerance_bps: None,
            score_units: score(8_000),
        })
        .expect("valid fixed decimals compare");
    assert!(
        absolute_boundary.is_some(),
        "absolute tolerance equality is inclusive",
    );

    let relative_hit = numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
        namespace: "neutral.structured",
        operator_id: "numeric_within_tolerance:reported_amount",
        reason_code: "amount_within_relative_tolerance",
        view_name: "reported_amount",
        left_value: "100.00",
        right_value: "100.04",
        absolute_tolerance: "0",
        relative_tolerance_bps: Some(5),
        score_units: score(7_500),
    })
    .expect("valid explicit relative tolerance compares")
    .expect("relative basis-point tolerance supports");
    assert_eq!(relative_hit.score_units, score(7_500));

    let relative_boundary =
        numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
            namespace: "neutral.structured",
            operator_id: "numeric_within_tolerance:reported_amount",
            reason_code: "amount_within_relative_tolerance",
            view_name: "reported_amount",
            left_value: "100.00",
            right_value: "99.00",
            absolute_tolerance: "0",
            relative_tolerance_bps: Some(100),
            score_units: score(7_500),
        })
        .expect("valid explicit relative tolerance compares");
    assert!(
        relative_boundary.is_some(),
        "relative basis-point equality is inclusive",
    );

    let near_miss = numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
        namespace: "neutral.structured",
        operator_id: "numeric_within_tolerance:reported_amount",
        reason_code: "amount_within_absolute_tolerance",
        view_name: "reported_amount",
        left_value: "100.00",
        right_value: "100.06",
        absolute_tolerance: "0.05",
        relative_tolerance_bps: None,
        score_units: score(8_000),
    })
    .expect("valid fixed decimals compare");
    assert!(near_miss.is_none());

    let unit_mismatch =
        numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
            namespace: "neutral.structured",
            operator_id: "numeric_within_tolerance:reported_amount",
            reason_code: "amount_within_absolute_tolerance",
            view_name: "reported_amount",
            left_value: "1",
            right_value: "1000",
            absolute_tolerance: "0",
            relative_tolerance_bps: None,
            score_units: score(8_000),
        })
        .expect("different units are just different declared values");
    assert!(unit_mismatch.is_none());

    let malformed = numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
        namespace: "neutral.structured",
        operator_id: "numeric_within_tolerance:reported_amount",
        reason_code: "amount_within_absolute_tolerance",
        view_name: "reported_amount",
        left_value: "1e3",
        right_value: "1000",
        absolute_tolerance: "0",
        relative_tolerance_bps: None,
        score_units: score(8_000),
    })
    .expect_err("scientific notation is not an implicit conversion path");
    assert_eq!(malformed.field(), "left_value");
    assert_eq!(malformed.reason(), "malformed_decimal");

    let missing = numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
        namespace: "neutral.structured",
        operator_id: "numeric_within_tolerance:reported_amount",
        reason_code: "amount_within_absolute_tolerance",
        view_name: "reported_amount",
        left_value: "",
        right_value: "1000",
        absolute_tolerance: "0",
        relative_tolerance_bps: None,
        score_units: score(8_000),
    })
    .expect("missing values fail closed without support");
    assert!(missing.is_none());

    let parse_overflow =
        numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
            namespace: "neutral.structured",
            operator_id: "numeric_within_tolerance:reported_amount",
            reason_code: "amount_within_absolute_tolerance",
            view_name: "reported_amount",
            left_value: "9999999999999999999999999999999999999999",
            right_value: "0",
            absolute_tolerance: "0",
            relative_tolerance_bps: None,
            score_units: score(8_000),
        })
        .expect_err("oversized mantissa refuses deterministically");
    assert_eq!(parse_overflow.field(), "left_value");
    assert_eq!(parse_overflow.reason(), "decimal_overflow");

    let alignment_overflow =
        numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
            namespace: "neutral.structured",
            operator_id: "numeric_within_tolerance:reported_amount",
            reason_code: "amount_within_absolute_tolerance",
            view_name: "reported_amount",
            left_value: "1",
            right_value: "0.000000000000000000000000000000000000001",
            absolute_tolerance: "0",
            relative_tolerance_bps: None,
            score_units: score(8_000),
        })
        .expect_err("decimal scale alignment refuses deterministically");
    assert_eq!(alignment_overflow.field(), "numeric_difference");
    assert_eq!(alignment_overflow.reason(), "decimal_overflow");
}

#[test]
fn structured_support_date_and_categorical_boundaries_are_stable() {
    let exact = date_exact_support_hit(DateExactSupportRequest {
        namespace: "neutral.structured",
        operator_id: "date_exact:effective_date",
        reason_code: "date_exact_support",
        view_name: "effective_date",
        left_value: "2026-07-13",
        right_value: "2026-07-13",
        score_units: score(6_000),
    })
    .expect("valid ISO dates compare")
    .expect("same ISO date supports");
    assert_eq!(exact.score_units, score(6_000));

    let leap_near = date_near_support_hit(DateNearSupportRequest {
        namespace: "neutral.structured",
        operator_id: "date_near:effective_date",
        reason_code: "date_near_support",
        view_name: "effective_date",
        left_value: "2024-02-28",
        right_value: "2024-03-01",
        max_days: 2,
        score_units: score(5_500),
    })
    .expect("valid leap-year dates compare")
    .expect("two-day leap-year distance supports");
    assert_eq!(leap_near.score_units, score(5_500));

    let too_far = date_near_support_hit(DateNearSupportRequest {
        namespace: "neutral.structured",
        operator_id: "date_near:effective_date",
        reason_code: "date_near_support",
        view_name: "effective_date",
        left_value: "2024-02-28",
        right_value: "2024-03-01",
        max_days: 1,
        score_units: score(5_500),
    })
    .expect("valid leap-year dates compare");
    assert!(too_far.is_none());

    let invalid = date_exact_support_hit(DateExactSupportRequest {
        namespace: "neutral.structured",
        operator_id: "date_exact:effective_date",
        reason_code: "date_exact_support",
        view_name: "effective_date",
        left_value: "2026-02-30",
        right_value: "2026-02-28",
        score_units: score(6_000),
    })
    .expect_err("invalid calendar dates fail closed");
    assert_eq!(invalid.reason(), "invalid_date");

    let category = categorical_match_support_hit(CategoricalMatchSupportRequest {
        namespace: "neutral.structured",
        operator_id: "categorical_match:instrument_class",
        reason_code: "category_support",
        view_name: "instrument_class",
        left_value: "senior",
        right_value: "senior",
        score_units: score(4_000),
    })
    .expect("exact supplied category supports");
    assert_eq!(category.lane, ScoreLane::Support);

    assert!(
        categorical_match_support_hit(CategoricalMatchSupportRequest {
            namespace: "neutral.structured",
            operator_id: "categorical_match:instrument_class",
            reason_code: "category_support",
            view_name: "instrument_class",
            left_value: "senior",
            right_value: " senior ",
            score_units: score(4_000),
        })
        .is_none(),
        "categorical support does not add implicit whitespace normalization",
    );

    assert!(
        categorical_match_support_hit(CategoricalMatchSupportRequest {
            namespace: "neutral.structured",
            operator_id: "categorical_match:instrument_class",
            reason_code: "category_support",
            view_name: "instrument_class",
            left_value: "senior",
            right_value: "Senior",
            score_units: score(4_000),
        })
        .is_none(),
        "categorical support does not add implicit case normalization",
    );
}

#[test]
fn structured_support_uses_existing_score_and_cannot_link_authority() {
    let support = numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
        namespace: "neutral.structured",
        operator_id: "numeric_within_tolerance:reported_amount",
        reason_code: "amount_within_absolute_tolerance",
        view_name: "reported_amount",
        left_value: "42.00",
        right_value: "42.00",
        absolute_tolerance: "0",
        relative_tolerance_bps: None,
        score_units: score(8_500),
    })
    .expect("valid fixed decimals compare")
    .expect("exact numeric value supports");
    let cannot_link = EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        "neutral.guard",
        "conflicting_anchor:guard",
        "conflicting_anchor",
        score(10_000),
        true,
        "hard cannot-link authority remains separate",
    );
    let record = build_edge_evidence_record("surf:left", "surf:right", vec![support, cannot_link])
        .expect("edge evidence record builds");

    assert!(record.has_hard_cannot_link);
    assert_eq!(record.pair_score_total, score(8_500));
    assert_eq!(record.score_breakdown.raw_support_score_units, 8_500);
    assert_eq!(
        record
            .hits
            .iter()
            .filter(|hit| hit.lane == ScoreLane::Support)
            .count(),
        1
    );
}

#[test]
fn structured_support_numeric_tolerance_parameters_fail_closed() {
    validate_numeric_tolerance_params("0").expect("zero tolerance is explicit exact numeric");
    validate_numeric_tolerance_params("0.0001").expect("decimal tolerance is explicit");

    let malformed = validate_numeric_tolerance_params("1e-6")
        .expect_err("scientific notation is not a tolerance contract");
    assert_eq!(malformed.field(), "absolute_tolerance");
    assert_eq!(malformed.reason(), "malformed_decimal");

    let negative = validate_numeric_tolerance_params("-0.01")
        .expect_err("negative absolute tolerance refuses");
    assert_eq!(negative.field(), "absolute_tolerance");
    assert_eq!(negative.reason(), "negative_decimal");
}

#[test]
fn leading_dot_decimals_preserve_numeric_and_conflict_boundaries() {
    use canon::entity::anti_merge::{
        AttributeConflictComparison, AttributeConflictRequest, BasisPointScale,
        attribute_conflict_hit,
    };
    let conflict = |left, right| {
        attribute_conflict_hit(AttributeConflictRequest {
            namespace: "generic",
            operator_id: "attribute_conflict:rate",
            reason_code: "rate_conflict",
            field: "rate",
            left_value: left,
            right_value: right,
            comparison: AttributeConflictComparison::DecimalBasisPoints {
                scale: BasisPointScale::Percent,
            },
            tolerance_bps: 1,
            score_units: score(10_000),
        })
    };
    assert!(conflict(".625000000000", "0.625").unwrap().is_none());
    assert!(conflict("-.625", "-0.625").unwrap().is_none());
    assert!(conflict(".625", ".635").unwrap().is_none());
    assert!(conflict(".625", ".6351").unwrap().is_some());
    for malformed in [".", "-.", "1e-3", ".6x"] {
        assert_eq!(
            conflict(malformed, "0.625").unwrap_err().reason(),
            "malformed_decimal"
        );
    }
    let support = |left| {
        numeric_within_tolerance_support_hit(NumericWithinToleranceSupportRequest {
            namespace: "generic",
            operator_id: "numeric_within_tolerance:rate",
            reason_code: "rate_equal",
            view_name: "rate",
            left_value: left,
            right_value: "0.625",
            absolute_tolerance: "0",
            relative_tolerance_bps: None,
            score_units: score(8_000),
        })
    };
    assert!(support(".625000000000").unwrap().is_some());
    assert!(support(".6251").unwrap().is_none());
    assert_eq!(support(".").unwrap_err().reason(), "malformed_decimal");
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}

#[test]
fn declared_attribute_uncertainty_softens_only_the_named_cases() {
    use canon::entity::anti_merge::{
        AttributeConflictComparison as Comparison, AttributeConflictOptions,
        AttributeConflictRequest, BasisPointScale, attribute_conflict_hit_with_options,
    };
    let options = AttributeConflictOptions {
        unit_ambiguous: true,
        zero_is_unknown: true,
        ..Default::default()
    };
    let compare = |left, right, comparison, options: &AttributeConflictOptions| {
        attribute_conflict_hit_with_options(
            AttributeConflictRequest {
                namespace: "generic",
                operator_id: "attribute_conflict:x",
                reason_code: "attribute_conflict",
                field: "x",
                left_value: left,
                right_value: right,
                comparison,
                tolerance_bps: 1,
                score_units: score(10_000),
            },
            options,
        )
    };
    let rate = Comparison::DecimalBasisPoints {
        scale: BasisPointScale::Percent,
    };
    for (left, right, reason) in [
        ("4.125", ".04125", "unit_ambiguous"),
        (".00632", ".63", "unit_ambiguous"),
        ("0", "4.125", "zero_is_unknown"),
    ] {
        for (a, b) in [(left, right), (right, left)] {
            let hit = compare(a, b, rate, &options).unwrap().unwrap();
            assert!(!hit.hard_cannot_link);
            assert_eq!(hit.lane, canon::entity::score::ScoreLane::AntiMerge);
            assert!(hit.explanation.contains(reason));
            assert!(
                compare(a, b, rate, &AttributeConflictOptions::default())
                    .unwrap()
                    .unwrap()
                    .hard_cannot_link
            );
        }
    }
    assert!(
        compare("4.125", "4.50", rate, &options)
            .unwrap()
            .unwrap()
            .hard_cannot_link
    );
    assert!(
        !compare(".04125", ".0450", rate, &options)
            .unwrap()
            .unwrap()
            .hard_cannot_link
    );
    assert!(compare("4.125", "4.125", rate, &options).unwrap().is_none());
    assert!(compare("1e-3", "1", rate, &options).is_err());
    assert!(options.validate(Comparison::Date).is_err());
    let options = AttributeConflictOptions {
        sentinel_dates: vec!["2080-01-01".into()],
        sentinel_year_min: Some(2099),
        ..Default::default()
    };
    for (date, reason) in [
        ("2099-12-31", "sentinel_year_min"),
        ("2500-12-31", "sentinel_year_min"),
        ("2080-01-01", "sentinel_dates"),
    ] {
        let hit = compare(date, "2030-01-01", Comparison::Date, &options)
            .unwrap()
            .unwrap();
        assert!(!hit.hard_cannot_link);
        assert!(hit.explanation.contains(reason));
    }
    assert!(
        compare("2030-01-01", "2031-01-01", Comparison::Date, &options)
            .unwrap()
            .unwrap()
            .hard_cannot_link
    );
    assert!(compare("2099-02-30", "2030-01-01", Comparison::Date, &options).is_err());
}
