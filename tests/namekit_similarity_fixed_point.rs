use canon::namekit::{
    SimilarityScore,
    similarity::{
        SimilarityMetric, SimilarityOptions, SimilarityPath, batch_normalized_similarity,
        normalized_similarity, score_units_from_ratio,
    },
};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct MetricFixture {
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
fn namekit_similarity_fixed_point() {
    assert_eq!(score_units_from_ratio(-1.0).as_scaled(), 0);
    assert_eq!(score_units_from_ratio(0.0).as_scaled(), 0);
    assert_eq!(score_units_from_ratio(0.000_04).as_scaled(), 0);
    assert_eq!(score_units_from_ratio(0.000_05).as_scaled(), 1);
    assert_eq!(score_units_from_ratio(0.818_181_818).as_scaled(), 8182);
    assert_eq!(score_units_from_ratio(0.999_95).as_scaled(), 10_000);
    assert_eq!(score_units_from_ratio(2.0).as_scaled(), 10_000);
    assert_eq!(score_units_from_ratio(f64::NAN).as_scaled(), 0);

    let below_cutoff = normalized_similarity(
        SimilarityMetric::LevenshteinNormalized,
        "South Korea",
        "North Korea",
        SimilarityOptions::new(Some(score(9_000)), Some(score(8_000))),
    );
    assert_eq!(below_cutoff.score, None);
    assert!(!below_cutoff.passed_cutoff);
    assert_eq!(
        below_cutoff.score_cutoff.map(SimilarityScore::as_scaled),
        Some(9_000)
    );
    assert_eq!(
        below_cutoff.score_hint.map(SimilarityScore::as_scaled),
        Some(8_000)
    );
    assert!(below_cutoff.evidence_only);

    let no_cutoff = normalized_similarity(
        SimilarityMetric::LevenshteinNormalized,
        "South Korea",
        "North Korea",
        SimilarityOptions::default(),
    );
    assert_eq!(no_cutoff.score.map(SimilarityScore::as_scaled), Some(8_182));
    assert!(no_cutoff.passed_cutoff);
}

#[test]
fn namekit_metric_parity() {
    for fixture in metric_fixtures() {
        let options = fixture.options();
        let result =
            normalized_similarity(fixture.metric(), &fixture.left, &fixture.right, options);
        assert_eq!(
            result.path,
            fixture.expected_path(),
            "{} path changed",
            fixture.case_id
        );
        assert_eq!(
            result.score.map(SimilarityScore::as_scaled),
            fixture.expected_score_units,
            "{} score units changed",
            fixture.case_id
        );
        assert_eq!(
            result.passed_cutoff, fixture.expect_pass,
            "{} cutoff behavior changed",
            fixture.case_id
        );
        assert_eq!(
            result.evidence_only, fixture.evidence_only,
            "{} metrics must remain evidence only",
            fixture.case_id
        );

        let unhinted = normalized_similarity(
            fixture.metric(),
            &fixture.left,
            &fixture.right,
            SimilarityOptions::new(options.score_cutoff, None),
        );
        assert_eq!(
            unhinted.score.map(SimilarityScore::as_scaled),
            result.score.map(SimilarityScore::as_scaled),
            "{} score_hint changed score units",
            fixture.case_id
        );
        assert_eq!(
            unhinted.passed_cutoff, result.passed_cutoff,
            "{} score_hint changed cutoff pass/fail",
            fixture.case_id
        );

        if let Some(expected_without_cutoff) = fixture.expected_without_cutoff_units {
            let without_cutoff = normalized_similarity(
                fixture.metric(),
                &fixture.left,
                &fixture.right,
                SimilarityOptions::default(),
            );
            assert_eq!(
                without_cutoff.score.map(SimilarityScore::as_scaled),
                Some(expected_without_cutoff),
                "{} no-cutoff score units changed",
                fixture.case_id
            );
        }

        if fixture.batch_reuse {
            let batch = batch_normalized_similarity(
                fixture.metric(),
                &fixture.left,
                &[fixture.right.as_str()],
                options,
            );
            assert_eq!(
                batch.len(),
                1,
                "{} batch result count changed",
                fixture.case_id
            );
            assert!(
                batch[0].batch_reused,
                "{} batch comparator was not used",
                fixture.case_id
            );
            assert_eq!(
                batch[0].score.map(SimilarityScore::as_scaled),
                result.score.map(SimilarityScore::as_scaled),
                "{} batch score units changed",
                fixture.case_id
            );
        }
    }
}

impl MetricFixture {
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

fn metric_fixtures() -> Vec<MetricFixture> {
    fs::read_to_string("tests/fixtures/namekit/source_parity/rapidfuzz_metrics.jsonl")
        .expect("rapidfuzz metric fixture is readable")
        .lines()
        .map(|line| serde_json::from_str(line).expect("rapidfuzz metric fixture line is valid"))
        .collect()
}

fn score(units: u16) -> SimilarityScore {
    SimilarityScore::from_scaled(units).expect("fixture score units are within namekit scale")
}
