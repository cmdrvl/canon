use canon::entity::score::ScoreUnits;
use canon::namekit::{
    SimilarityScore,
    similarity::{
        SimilarityMetric, SimilarityOptions, SimilarityPath, batch_normalized_similarity,
        normalized_similarity, score_units_from_ratio,
    },
};
use serde::Deserialize;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;

const PARITY_FIXTURE: &str = "tests/fixtures/namekit/metrics/parity.jsonl";
const ORDERING_FIXTURE: &str = "tests/fixtures/namekit/metrics/score_ordering.jsonl";

#[derive(Debug, Deserialize)]
struct MetricParityFixture {
    case_id: String,
    metric: String,
    left: String,
    right: String,
    expected_path: String,
    expected_score_units: Option<u16>,
    expected_without_cutoff_units: u16,
    cutoff_units: Option<u16>,
    hint_units: Option<u16>,
    expect_pass: bool,
    expect_symmetric: bool,
    expected_edge_score_units: u32,
}

#[derive(Debug, Deserialize)]
struct OrderingFixture {
    ordering_group: String,
    rank: usize,
    metric: String,
    left: String,
    surface_id: String,
    right: String,
    expected_score_units: u32,
}

#[test]
fn namekit_metric_parity() {
    let fixtures = parity_fixtures();
    assert!(
        fixtures.len() >= 10,
        "metric parity fixture must cover all selected metrics"
    );

    for fixture in fixtures {
        let metric = fixture.metric();
        let options = fixture.options();
        let first = normalized_similarity(metric, &fixture.left, &fixture.right, options);
        let second = normalized_similarity(metric, &fixture.left, &fixture.right, options);
        let third = normalized_similarity(metric, &fixture.left, &fixture.right, options);

        assert_eq!(first, second, "{} changed on second run", fixture.case_id);
        assert_eq!(first, third, "{} changed on third run", fixture.case_id);
        assert_eq!(
            first.path,
            fixture.expected_path(),
            "{} path changed",
            fixture.case_id
        );
        assert_eq!(
            first.score.map(SimilarityScore::as_scaled),
            fixture.expected_score_units,
            "{} score units changed",
            fixture.case_id
        );
        assert_eq!(
            first.passed_cutoff, fixture.expect_pass,
            "{} cutoff pass/fail changed",
            fixture.case_id
        );
        assert!(
            first.evidence_only,
            "{} metric score must remain support evidence only",
            fixture.case_id
        );

        let unhinted = normalized_similarity(
            metric,
            &fixture.left,
            &fixture.right,
            SimilarityOptions::new(options.score_cutoff, None),
        );
        assert_eq!(
            unhinted.score.map(SimilarityScore::as_scaled),
            first.score.map(SimilarityScore::as_scaled),
            "{} score_hint changed score units",
            fixture.case_id
        );
        assert_eq!(
            unhinted.passed_cutoff, first.passed_cutoff,
            "{} score_hint changed cutoff decision",
            fixture.case_id
        );

        let without_cutoff = normalized_similarity(
            metric,
            &fixture.left,
            &fixture.right,
            SimilarityOptions::default(),
        );
        assert_eq!(
            without_cutoff.score.map(SimilarityScore::as_scaled),
            Some(fixture.expected_without_cutoff_units),
            "{} no-cutoff score changed",
            fixture.case_id
        );

        let batch =
            batch_normalized_similarity(metric, &fixture.left, &[fixture.right.as_str()], options);
        assert_eq!(
            batch.len(),
            1,
            "{} batch result count changed",
            fixture.case_id
        );
        assert!(
            batch[0].batch_reused,
            "{} batch comparator marker changed",
            fixture.case_id
        );
        assert_eq!(
            batch[0].path, first.path,
            "{} batch path changed",
            fixture.case_id
        );
        assert_eq!(
            batch[0].score.map(SimilarityScore::as_scaled),
            first.score.map(SimilarityScore::as_scaled),
            "{} batch score units changed",
            fixture.case_id
        );
        assert_eq!(
            batch[0].passed_cutoff, first.passed_cutoff,
            "{} batch cutoff decision changed",
            fixture.case_id
        );

        if fixture.expect_symmetric {
            let reversed = normalized_similarity(
                metric,
                &fixture.right,
                &fixture.left,
                SimilarityOptions::default(),
            );
            assert_eq!(
                reversed.score.map(SimilarityScore::as_scaled),
                Some(fixture.expected_without_cutoff_units),
                "{} reversed score changed",
                fixture.case_id
            );
        }
    }
}

#[test]
fn deterministic_score_units() {
    assert_eq!(score_units_from_ratio(-1.0).as_scaled(), 0);
    assert_eq!(score_units_from_ratio(0.000_04).as_scaled(), 0);
    assert_eq!(score_units_from_ratio(0.000_05).as_scaled(), 1);
    assert_eq!(score_units_from_ratio(0.818_181_818).as_scaled(), 8_182);
    assert_eq!(score_units_from_ratio(0.999_95).as_scaled(), 10_000);
    assert_eq!(score_units_from_ratio(f64::NAN).as_scaled(), 0);

    for fixture in parity_fixtures() {
        let score = SimilarityScore::from_scaled(fixture.expected_without_cutoff_units)
            .expect("fixture score units are within namekit scale");
        let edge_units = ScoreUnits::from(score);
        assert_eq!(
            edge_units.as_u32(),
            fixture.expected_edge_score_units,
            "{} edge score conversion changed",
            fixture.case_id
        );
    }

    for (group, mut rows) in ordering_groups() {
        rows.sort_by_key(|row| row.rank);
        let expected = rows
            .iter()
            .map(|row| row.surface_id.as_str())
            .collect::<Vec<_>>();

        let mut actual = rows
            .iter()
            .map(|row| {
                let result = normalized_similarity(
                    row.metric(),
                    &row.left,
                    &row.right,
                    SimilarityOptions::default(),
                );
                let score_units = ScoreUnits::from(
                    result
                        .score
                        .expect("ordering fixtures must produce a score"),
                );
                assert_eq!(
                    score_units.as_u32(),
                    row.expected_score_units,
                    "{} fixture score changed for {}",
                    group,
                    row.surface_id
                );
                (row.surface_id.as_str(), score_units)
            })
            .collect::<Vec<_>>();

        actual
            .sort_by_key(|(surface_id, score_units)| (Reverse(score_units.as_u32()), *surface_id));
        let actual = actual
            .iter()
            .map(|(surface_id, _)| *surface_id)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{group} edge ordering changed");
    }
}

impl MetricParityFixture {
    fn metric(&self) -> SimilarityMetric {
        parse_metric(&self.metric)
    }

    fn expected_path(&self) -> SimilarityPath {
        parse_path(&self.expected_path)
    }

    fn options(&self) -> SimilarityOptions {
        SimilarityOptions::new(self.cutoff_units.map(score), self.hint_units.map(score))
    }
}

impl OrderingFixture {
    fn metric(&self) -> SimilarityMetric {
        parse_metric(&self.metric)
    }
}

fn parity_fixtures() -> Vec<MetricParityFixture> {
    jsonl_rows(PARITY_FIXTURE)
}

fn ordering_groups() -> BTreeMap<String, Vec<OrderingFixture>> {
    let mut groups = BTreeMap::<String, Vec<OrderingFixture>>::new();
    for row in jsonl_rows::<OrderingFixture>(ORDERING_FIXTURE) {
        groups
            .entry(row.ordering_group.clone())
            .or_default()
            .push(row);
    }
    groups
}

fn jsonl_rows<T>(path: &str) -> Vec<T>
where
    T: for<'de> Deserialize<'de>,
{
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{path} is readable: {error}"))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{path}: {error}")))
        .collect()
}

fn parse_metric(metric: &str) -> SimilarityMetric {
    match metric {
        "levenshtein_normalized" => SimilarityMetric::LevenshteinNormalized,
        "damerau_levenshtein_normalized" => SimilarityMetric::DamerauLevenshteinNormalized,
        "jaro_winkler" => SimilarityMetric::JaroWinkler,
        "dice_sorensen" => SimilarityMetric::DiceSorensen,
        "token_sort_ratio" => SimilarityMetric::TokenSortRatio,
        "token_set_ratio" => SimilarityMetric::TokenSetRatio,
        other => panic!("unexpected metric in fixture: {other}"),
    }
}

fn parse_path(path: &str) -> SimilarityPath {
    match path {
        "ascii_bytes" => SimilarityPath::AsciiBytes,
        "unicode_chars" => SimilarityPath::UnicodeChars,
        other => panic!("unexpected similarity path in fixture: {other}"),
    }
}

fn score(units: u16) -> SimilarityScore {
    SimilarityScore::from_scaled(units).expect("fixture score units are within namekit scale")
}
