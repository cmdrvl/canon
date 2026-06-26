#![forbid(unsafe_code)]

use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const REVIEW_QUEUE_PATH: &str = "tests/fixtures/entity/cmbs/small_book/review_queue.csv";
const SUMMARY_PATH: &str = "tests/fixtures/entity/cmbs/small_book/expected_summary.json";
const OBSERVATIONS_PATH: &str = "tests/fixtures/entity/cmbs/small_book/observations.csv";

#[derive(Debug, Deserialize)]
struct ReviewQueueRow {
    review_group_id: String,
    benchmark_id: String,
    reason_code: String,
    row_count: u64,
    deal_count: u64,
    property_count: u64,
    representative_surfaces_json: String,
    suggested_action: String,
}

#[derive(Debug, Deserialize)]
struct Summary {
    review_groups: Vec<SummaryReviewGroup>,
}

#[derive(Debug, Deserialize)]
struct SummaryReviewGroup {
    id: String,
    benchmark_id: String,
    reason_code: String,
    row_count: u64,
    deal_count: u64,
    property_count: u64,
    representative_surfaces: Vec<String>,
    suggested_action: String,
}

#[test]
fn review_grouping_cmbs_small_book_groups_ambiguities_not_rows() {
    let queue = read_review_queue();
    let summary: Summary = read_json(&repo_path(SUMMARY_PATH));
    let observations = read_observations();

    assert_eq!(queue.len(), 2);
    assert_eq!(queue.len(), summary.review_groups.len());
    assert_eq!(
        queue
            .iter()
            .map(|row| row.review_group_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        queue.len()
    );

    let review_row_count = observations
        .iter()
        .filter(|row| {
            row.get("expected_review_group")
                .is_some_and(|group| !group.trim().is_empty())
        })
        .count() as u64;
    assert_eq!(
        queue.iter().map(|row| row.row_count).sum::<u64>(),
        review_row_count
    );
    assert!(
        queue.len() < review_row_count as usize,
        "review queue must be grouped instead of row-level"
    );
    assert!(queue.iter().all(|row| row.row_count > 1));
    assert!(queue.iter().all(|row| row.deal_count == row.row_count));
    assert!(queue.iter().all(|row| row.property_count == row.row_count));

    let summary_by_id = summary
        .review_groups
        .iter()
        .map(|group| (group.id.as_str(), group))
        .collect::<BTreeMap<_, _>>();
    for row in &queue {
        let summary_group = summary_by_id
            .get(row.review_group_id.as_str())
            .expect("summary group exists");
        assert_eq!(row.benchmark_id, summary_group.benchmark_id);
        assert_eq!(row.reason_code, summary_group.reason_code);
        assert_eq!(row.row_count, summary_group.row_count);
        assert_eq!(row.deal_count, summary_group.deal_count);
        assert_eq!(row.property_count, summary_group.property_count);
        assert_eq!(row.suggested_action, summary_group.suggested_action);
        assert_eq!(
            representative_surfaces(row),
            summary_group.representative_surfaces
        );
    }
}

fn representative_surfaces(row: &ReviewQueueRow) -> Vec<String> {
    serde_json::from_str(&row.representative_surfaces_json).expect("representatives json")
}

fn read_review_queue() -> Vec<ReviewQueueRow> {
    csv::Reader::from_path(repo_path(REVIEW_QUEUE_PATH))
        .expect("review queue opens")
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .expect("review queue parses")
}

fn read_observations() -> Vec<BTreeMap<String, String>> {
    csv::Reader::from_path(repo_path(OBSERVATIONS_PATH))
        .expect("observations open")
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .expect("observations parse")
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
