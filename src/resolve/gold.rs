use super::{GoldScore, MatchDecisions, ResolveError, ResolveErrorCode, ResolveResult};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldSet {
    entries: BTreeMap<String, String>,
}

impl GoldSet {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Deserialize)]
struct GoldJsonlRecord {
    target_id: String,
    expected_reference_id: String,
}

pub fn load_gold(path: &Path) -> ResolveResult<GoldSet> {
    let bytes = fs::read(path).map_err(|error| {
        ResolveError::with_detail(
            ResolveErrorCode::Io,
            format!("Unable to read gold set '{}': {error}", path.display()),
            json!({
                "gold": path.display().to_string(),
                "error": error.to_string()
            }),
        )
    })?;
    parse_gold_jsonl(&bytes)
}

pub fn parse_gold_jsonl(bytes: &[u8]) -> ResolveResult<GoldSet> {
    let content = std::str::from_utf8(bytes).map_err(|error| {
        gold_error(
            format!("Gold set must be UTF-8 JSONL: {error}"),
            json!({ "reason": error.to_string() }),
        )
    })?;

    let mut entries = BTreeMap::new();
    for (line_index, line) in content.lines().enumerate() {
        let line_number = line_index + 1;
        if line.trim().is_empty() {
            continue;
        }

        let record: GoldJsonlRecord = serde_json::from_str(line).map_err(|error| {
            gold_error(
                format!("Invalid gold JSONL on line {line_number}: {error}"),
                json!({
                    "line": line_number,
                    "reason": error.to_string()
                }),
            )
        })?;

        let target_id = record.target_id.trim();
        let expected_reference_id = record.expected_reference_id.trim();
        if target_id.is_empty() || expected_reference_id.is_empty() {
            return Err(gold_error(
                format!("Gold JSONL line {line_number} has an empty target or reference ID"),
                json!({
                    "line": line_number,
                    "target_id": record.target_id,
                    "expected_reference_id": record.expected_reference_id
                }),
            ));
        }

        if entries
            .insert(target_id.to_string(), expected_reference_id.to_string())
            .is_some()
        {
            return Err(gold_error(
                format!("Gold set contains duplicate target_id '{target_id}'"),
                json!({
                    "line": line_number,
                    "target_id": target_id
                }),
            ));
        }
    }

    if entries.is_empty() {
        return Err(gold_error(
            "Gold set must contain at least one record",
            json!({ "record_count": 0 }),
        ));
    }

    Ok(GoldSet { entries })
}

pub fn score_gold(decisions: &MatchDecisions, gold: &GoldSet) -> GoldScore {
    let actual_matches = decisions
        .matches
        .iter()
        .map(|record| (record.target_id.clone(), record.reference_id.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut correct = 0;
    let mut incorrect = 0;
    let mut unmatched_in_gold = 0;
    let mut regressions = Vec::new();

    for (target_id, expected_reference_id) in &gold.entries {
        match actual_matches.get(target_id) {
            Some(actual_reference_id) if actual_reference_id == expected_reference_id => {
                correct += 1;
            }
            Some(_) => {
                incorrect += 1;
                regressions.push(target_id.clone());
            }
            None => {
                unmatched_in_gold += 1;
            }
        }
    }

    let total = gold.len();
    let accuracy = if total == 0 {
        0.0
    } else {
        correct as f64 / total as f64
    };

    GoldScore {
        total,
        correct,
        incorrect,
        unmatched_in_gold,
        accuracy,
        regressions,
    }
}

pub fn score_gold_file(path: &Path, decisions: &MatchDecisions) -> ResolveResult<GoldScore> {
    let gold = load_gold(path)?;
    Ok(score_gold(decisions, &gold))
}

fn gold_error(message: impl Into<String>, detail: serde_json::Value) -> ResolveError {
    ResolveError::with_detail(ResolveErrorCode::Gold, message, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::{AmbiguousRecord, MatchRecord, UnmatchedRecord};

    fn gold(input: &str) -> GoldSet {
        parse_gold_jsonl(input.as_bytes()).unwrap()
    }

    fn decisions(matches: &[(&str, &str)]) -> MatchDecisions {
        MatchDecisions {
            matches: matches
                .iter()
                .map(|(target_id, reference_id)| MatchRecord {
                    target_id: (*target_id).to_string(),
                    reference_id: (*reference_id).to_string(),
                    canonical_id: (*reference_id).to_string(),
                    score: 1.0,
                    assertions: vec![],
                    runner_up: None,
                })
                .collect(),
            ..MatchDecisions::default()
        }
    }

    #[test]
    fn parses_gold_jsonl_into_deterministic_target_map() {
        let gold = parse_gold_jsonl(
            br#"
{"target_id":"T-2","expected_reference_id":"R-2"}
{"target_id":"T-1","expected_reference_id":"R-1"}
"#,
        )
        .unwrap();

        assert_eq!(gold.len(), 2);
        assert_eq!(
            gold.entries.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["T-1", "T-2"]
        );
    }

    #[test]
    fn all_correct_gold_scores_perfect_accuracy() {
        let gold = gold(
            r#"
{"target_id":"T-1","expected_reference_id":"R-1"}
{"target_id":"T-2","expected_reference_id":"R-2"}
"#,
        );
        let score = score_gold(&decisions(&[("T-1", "R-1"), ("T-2", "R-2")]), &gold);

        assert_eq!(score.total, 2);
        assert_eq!(score.correct, 2);
        assert_eq!(score.incorrect, 0);
        assert_eq!(score.unmatched_in_gold, 0);
        assert_eq!(score.accuracy, 1.0);
        assert!(score.regressions.is_empty());
    }

    #[test]
    fn planted_wrong_match_is_regression() {
        let gold = gold(
            r#"
{"target_id":"T-1","expected_reference_id":"R-1"}
{"target_id":"T-2","expected_reference_id":"R-2"}
"#,
        );
        let score = score_gold(&decisions(&[("T-1", "R-9"), ("T-2", "R-2")]), &gold);

        assert_eq!(score.total, 2);
        assert_eq!(score.correct, 1);
        assert_eq!(score.incorrect, 1);
        assert_eq!(score.unmatched_in_gold, 0);
        assert_eq!(score.accuracy, 0.5);
        assert_eq!(score.regressions, vec!["T-1"]);
    }

    #[test]
    fn unmatched_gold_target_counts_as_unmatched_in_gold() {
        let gold = gold(
            r#"
{"target_id":"T-1","expected_reference_id":"R-1"}
{"target_id":"T-2","expected_reference_id":"R-2"}
"#,
        );
        let mut decisions = decisions(&[("T-1", "R-1")]);
        decisions.unmatched.push(UnmatchedRecord {
            target_id: "T-2".to_string(),
            reason: "no_candidates".to_string(),
            best_candidate: None,
        });

        let score = score_gold(&decisions, &gold);

        assert_eq!(score.correct, 1);
        assert_eq!(score.incorrect, 0);
        assert_eq!(score.unmatched_in_gold, 1);
        assert_eq!(score.accuracy, 0.5);
        assert!(score.regressions.is_empty());
    }

    #[test]
    fn ambiguous_gold_target_counts_as_unmatched_in_gold() {
        let gold = gold(r#"{"target_id":"T-1","expected_reference_id":"R-1"}"#);
        let mut decisions = MatchDecisions::default();
        decisions.ambiguous.push(AmbiguousRecord {
            target_id: "T-1".to_string(),
            candidates: vec![],
            gap: 0.0,
            reason: "insufficient_ambiguity_gap".to_string(),
        });

        let score = score_gold(&decisions, &gold);

        assert_eq!(score.correct, 0);
        assert_eq!(score.incorrect, 0);
        assert_eq!(score.unmatched_in_gold, 1);
        assert_eq!(score.accuracy, 0.0);
    }

    #[test]
    fn malformed_jsonl_maps_to_gold_error() {
        let error = parse_gold_jsonl(br#"{"target_id":"T-1""#).unwrap_err();

        assert_eq!(error.code, ResolveErrorCode::Gold);
        assert!(error.message.contains("Invalid gold JSONL"));
    }

    #[test]
    fn duplicate_target_ids_map_to_gold_error() {
        let error = parse_gold_jsonl(
            br#"
{"target_id":"T-1","expected_reference_id":"R-1"}
{"target_id":"T-1","expected_reference_id":"R-2"}
"#,
        )
        .unwrap_err();

        assert_eq!(error.code, ResolveErrorCode::Gold);
        assert!(error.message.contains("duplicate target_id"));
    }

    #[test]
    fn regressions_are_ordered_by_target_id() {
        let gold = gold(
            r#"
{"target_id":"T-2","expected_reference_id":"R-2"}
{"target_id":"T-1","expected_reference_id":"R-1"}
{"target_id":"T-3","expected_reference_id":"R-3"}
"#,
        );
        let score = score_gold(
            &decisions(&[("T-3", "BAD"), ("T-1", "BAD"), ("T-2", "BAD")]),
            &gold,
        );

        assert_eq!(score.regressions, vec!["T-1", "T-2", "T-3"]);
    }

    #[test]
    fn fixture_gold_files_parse_and_regression_fixture_scores_as_expected() {
        let clean = load_gold(Path::new("tests/fixtures/resolve/gold/loan_matches.jsonl")).unwrap();
        let planted = load_gold(Path::new(
            "tests/fixtures/resolve/gold/loan_matches_with_regression.jsonl",
        ))
        .unwrap();
        let decisions = decisions(&[
            ("WFCM2019-C50|1", "223232"),
            ("WFCM2019-C50|2", "223233"),
            ("WFCM2019-C50|3", "223234"),
            ("WFCM2019-C50|4", "223235"),
            ("WFCM2019-C50|5", "223236"),
            ("WFCM2019-C50|6", "223237"),
            ("WFCM2019-C50|7", "223238"),
            ("WFCM2019-C50|8", "223239"),
            ("WFCM2019-C50|9", "223240"),
        ]);

        let clean_score = score_gold(&decisions, &clean);
        assert_eq!(clean_score.correct, 9);
        assert_eq!(clean_score.accuracy, 1.0);

        let planted_score = score_gold(&decisions, &planted);
        assert_eq!(planted_score.correct, 8);
        assert_eq!(planted_score.incorrect, 1);
        assert_eq!(planted_score.regressions, vec!["WFCM2019-C50|9"]);
    }
}
