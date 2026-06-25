use canon::namekit::{
    SimilarityScore,
    similarity::{
        NAMEKIT_SIMILARITY_DECISION_VERSION, RAPIDFUZZ_CRATE, RAPIDFUZZ_VERSION, SimilarityMetric,
        SimilarityOptions, SimilarityPath, batch_normalized_similarity, normalized_similarity,
        score_units_from_ratio,
    },
};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct RapidFuzzFixture {
    case_id: String,
    metric: String,
    left: String,
    right: String,
    expected_path: String,
    expected_score_units: Option<u16>,
    expected_without_cutoff_units: Option<u16>,
    cutoff_units: Option<u16>,
    hint_units: Option<u16>,
    expect_pass: bool,
    batch_reuse: bool,
    evidence_only: bool,
}

#[test]
fn namekit_similarity_fixture_locks_rapidfuzz_contract() {
    assert_eq!(
        NAMEKIT_SIMILARITY_DECISION_VERSION,
        "canon_namekit_similarity.v0"
    );
    assert_eq!(RAPIDFUZZ_CRATE, "rapidfuzz");
    assert_eq!(RAPIDFUZZ_VERSION, "0.5.0");

    for fixture in rapidfuzz_fixtures() {
        let metric = fixture.metric();
        let options = fixture.options();
        let result = normalized_similarity(metric, &fixture.left, &fixture.right, options);

        assert_eq!(
            result.path,
            fixture.expected_path(),
            "{} path changed",
            fixture.case_id
        );
        assert_eq!(
            result.score.map(SimilarityScore::as_scaled),
            fixture.expected_score_units,
            "{} score changed",
            fixture.case_id
        );
        assert_eq!(
            result.passed_cutoff, fixture.expect_pass,
            "{} cutoff pass/fail changed",
            fixture.case_id
        );
        assert_eq!(
            result.evidence_only, fixture.evidence_only,
            "{} must remain support evidence only",
            fixture.case_id
        );

        if fixture.batch_reuse {
            let batch = batch_normalized_similarity(
                metric,
                &fixture.left,
                &[fixture.right.as_str()],
                options,
            );
            assert_eq!(batch.len(), 1, "{} batch size changed", fixture.case_id);
            assert!(
                batch[0].batch_reused,
                "{} batch path was not used",
                fixture.case_id
            );
            assert_eq!(
                batch[0].score.map(SimilarityScore::as_scaled),
                result.score.map(SimilarityScore::as_scaled),
                "{} batch score changed",
                fixture.case_id
            );
        }
    }
}

#[test]
fn rapidfuzz_cutoff_hint_parity() {
    for fixture in rapidfuzz_fixtures() {
        let metric = fixture.metric();
        let cutoff_options = fixture.options();
        let hinted = normalized_similarity(metric, &fixture.left, &fixture.right, cutoff_options);
        let unhinted = normalized_similarity(
            metric,
            &fixture.left,
            &fixture.right,
            SimilarityOptions::new(cutoff_options.score_cutoff, None),
        );
        assert_eq!(
            hinted.score.map(SimilarityScore::as_scaled),
            unhinted.score.map(SimilarityScore::as_scaled),
            "{} score_hint changed the answer",
            fixture.case_id
        );
        assert_eq!(
            hinted.passed_cutoff, unhinted.passed_cutoff,
            "{} score_hint changed cutoff semantics",
            fixture.case_id
        );

        let no_cutoff = normalized_similarity(
            metric,
            &fixture.left,
            &fixture.right,
            SimilarityOptions::default(),
        );
        if let Some(expected_without_cutoff) = fixture.expected_without_cutoff_units {
            assert_eq!(
                no_cutoff.score.map(SimilarityScore::as_scaled),
                Some(expected_without_cutoff),
                "{} no-cutoff score changed",
                fixture.case_id
            );
        }
        if !fixture.expect_pass {
            assert!(
                no_cutoff.score.is_some(),
                "{} cutoff must be the only reason the score is suppressed",
                fixture.case_id
            );
        }
    }
}

#[test]
fn metric_ascii_unicode_parity() {
    let ascii = normalized_similarity(
        SimilarityMetric::LevenshteinNormalized,
        "tenant",
        "tenants",
        SimilarityOptions::default(),
    );
    assert_eq!(ascii.path, SimilarityPath::AsciiBytes);
    assert_eq!(ascii.score.map(SimilarityScore::as_scaled), Some(8571));

    let unicode = normalized_similarity(
        SimilarityMetric::LevenshteinNormalized,
        "Cafe",
        "Café",
        SimilarityOptions::default(),
    );
    assert_eq!(unicode.path, SimilarityPath::UnicodeChars);
    assert_eq!(unicode.score.map(SimilarityScore::as_scaled), Some(7500));

    let folded_ascii = normalized_similarity(
        SimilarityMetric::LevenshteinNormalized,
        "Cafe",
        "Cafe",
        SimilarityOptions::default(),
    );
    assert_eq!(folded_ascii.path, SimilarityPath::AsciiBytes);
    assert_eq!(
        folded_ascii.score.map(SimilarityScore::as_scaled),
        Some(10000)
    );
}

#[test]
fn local_metric_variants_are_deterministic_evidence_only() {
    let dice = normalized_similarity(
        SimilarityMetric::DiceSorensen,
        "night",
        "nacht",
        SimilarityOptions::default(),
    );
    assert_eq!(dice.path, SimilarityPath::AsciiBytes);
    assert_eq!(dice.score.map(SimilarityScore::as_scaled), Some(2500));
    assert!(dice.evidence_only);

    let unicode_dice = normalized_similarity(
        SimilarityMetric::DiceSorensen,
        "éclair",
        "éclat",
        SimilarityOptions::default(),
    );
    assert_eq!(unicode_dice.path, SimilarityPath::UnicodeChars);
    assert_eq!(
        unicode_dice.score.map(SimilarityScore::as_scaled),
        Some(6667)
    );

    let token_sort = normalized_similarity(
        SimilarityMetric::TokenSortRatio,
        "roebuck sears",
        "sears roebuck",
        SimilarityOptions::default(),
    );
    assert_eq!(token_sort.path, SimilarityPath::AsciiBytes);
    assert_eq!(
        token_sort.score.map(SimilarityScore::as_scaled),
        Some(10_000)
    );

    let token_set = normalized_similarity(
        SimilarityMetric::TokenSetRatio,
        "sears roebuck",
        "sears roebuck llc",
        SimilarityOptions::default(),
    );
    assert_eq!(token_set.score.map(SimilarityScore::as_scaled), Some(8000));
    assert!(token_set.evidence_only);
}

#[test]
fn local_metric_cutoffs_empty_and_long_inputs_are_stable() {
    let empty_pair = normalized_similarity(
        SimilarityMetric::TokenSetRatio,
        "",
        "",
        SimilarityOptions::default(),
    );
    assert_eq!(
        empty_pair.score.map(SimilarityScore::as_scaled),
        Some(10_000)
    );

    let empty_side = normalized_similarity(
        SimilarityMetric::DiceSorensen,
        "",
        "sears",
        SimilarityOptions::new(Some(score(1)), Some(score(5_000))),
    );
    assert_eq!(empty_side.score, None);
    assert!(!empty_side.passed_cutoff);
    assert!(empty_side.evidence_only);

    let long_left = format!("{} {}", "sears ".repeat(64), "roebuck");
    let long_right = format!("roebuck {}", "sears ".repeat(64));
    let long_score = normalized_similarity(
        SimilarityMetric::TokenSortRatio,
        &long_left,
        &long_right,
        SimilarityOptions::new(Some(score(10_000)), Some(score(9_000))),
    );
    assert_eq!(
        long_score.score.map(SimilarityScore::as_scaled),
        Some(10_000)
    );
    assert!(long_score.passed_cutoff);
}

#[test]
fn batch_comparator_reuse_matches_pairwise_scores() {
    let rights = ["Sears Roebuck", "Sears Outlet", "Roebuck Sears"];
    let options = SimilarityOptions::new(Some(score(7_000)), Some(score(8_000)));
    let batch = batch_normalized_similarity(
        SimilarityMetric::JaroWinkler,
        "Sears Roebuck",
        &rights,
        options,
    );
    let pairwise = rights
        .iter()
        .map(|right| {
            normalized_similarity(
                SimilarityMetric::JaroWinkler,
                "Sears Roebuck",
                right,
                options,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(batch.len(), pairwise.len());
    for (batch_result, pairwise_result) in batch.iter().zip(pairwise.iter()) {
        assert!(batch_result.batch_reused);
        assert_eq!(
            batch_result.score.map(SimilarityScore::as_scaled),
            pairwise_result.score.map(SimilarityScore::as_scaled)
        );
        assert_eq!(batch_result.passed_cutoff, pairwise_result.passed_cutoff);
        assert!(batch_result.evidence_only);
    }
}

#[test]
fn token_metric_batch_matches_pairwise_scores() {
    let rights = ["roebuck sears", "kmart stores"];
    let options = SimilarityOptions::new(Some(score(9_000)), Some(score(9_500)));
    let batch = batch_normalized_similarity(
        SimilarityMetric::TokenSortRatio,
        "sears roebuck",
        &rights,
        options,
    );
    let pairwise = rights
        .iter()
        .map(|right| {
            normalized_similarity(
                SimilarityMetric::TokenSortRatio,
                "sears roebuck",
                right,
                options,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(batch.len(), pairwise.len());
    assert_eq!(batch[0].score.map(SimilarityScore::as_scaled), Some(10_000));
    assert_eq!(batch[1].score, None);
    for (batch_result, pairwise_result) in batch.iter().zip(pairwise.iter()) {
        assert!(batch_result.batch_reused);
        assert_eq!(
            batch_result.score.map(SimilarityScore::as_scaled),
            pairwise_result.score.map(SimilarityScore::as_scaled)
        );
        assert_eq!(batch_result.passed_cutoff, pairwise_result.passed_cutoff);
        assert!(batch_result.evidence_only);
    }
}

#[test]
fn metric_scores_round_to_canon_integer_units() {
    assert_eq!(score_units_from_ratio(-0.1).as_scaled(), 0);
    assert_eq!(score_units_from_ratio(0.571_428_571).as_scaled(), 5714);
    assert_eq!(score_units_from_ratio(0.961_111_111).as_scaled(), 9611);
    assert_eq!(score_units_from_ratio(1.7).as_scaled(), 10_000);
    assert_eq!(score_units_from_ratio(f64::NAN).as_scaled(), 0);
}

impl RapidFuzzFixture {
    fn metric(&self) -> SimilarityMetric {
        match self.metric.as_str() {
            "levenshtein_normalized" => SimilarityMetric::LevenshteinNormalized,
            "jaro_winkler" => SimilarityMetric::JaroWinkler,
            "dice_sorensen" => SimilarityMetric::DiceSorensen,
            "token_sort_ratio" => SimilarityMetric::TokenSortRatio,
            "token_set_ratio" => SimilarityMetric::TokenSetRatio,
            other => panic!("unexpected metric in fixture: {other}"),
        }
    }

    fn expected_path(&self) -> SimilarityPath {
        match self.expected_path.as_str() {
            "ascii_bytes" => SimilarityPath::AsciiBytes,
            "unicode_chars" => SimilarityPath::UnicodeChars,
            other => panic!("unexpected path in fixture: {other}"),
        }
    }

    fn options(&self) -> SimilarityOptions {
        SimilarityOptions::new(self.cutoff_units.map(score), self.hint_units.map(score))
    }
}

fn rapidfuzz_fixtures() -> Vec<RapidFuzzFixture> {
    let fixture =
        fs::read_to_string("tests/fixtures/namekit/source_parity/rapidfuzz_metrics.jsonl")
            .expect("rapidfuzz fixture is readable");
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("rapidfuzz fixture line is valid JSON"))
        .collect()
}

fn score(units: u16) -> SimilarityScore {
    SimilarityScore::from_scaled(units).expect("fixture score units are in range")
}
