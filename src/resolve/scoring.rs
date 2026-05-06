use super::{
    AmbiguousRecord, CandidateScore, CandidateSelection, MatchRecord, ResolveStrategy,
    UnmatchedRecord, evaluate_assertion,
};
use crate::Registry;
use std::{cmp::Ordering, collections::BTreeMap};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MatchDecisions {
    pub matches: Vec<MatchRecord>,
    pub unmatched: Vec<UnmatchedRecord>,
    pub ambiguous: Vec<AmbiguousRecord>,
    pub conflict_warnings: Vec<String>,
}

pub fn score_candidates(
    selection: &CandidateSelection,
    strategy: &ResolveStrategy,
    registry: Option<&Registry>,
) -> MatchDecisions {
    let mut decisions = MatchDecisions::default();

    for target in &selection.targets {
        let target_record = selection.graph.record(target.target_node);
        let mut scored = target
            .candidates
            .iter()
            .map(|candidate| {
                let reference = selection.graph.record(candidate.reference_node);
                score_candidate_pair(
                    &candidate.reference_id,
                    reference,
                    target_record,
                    strategy,
                    registry,
                )
            })
            .collect::<Vec<_>>();

        scored.sort_by(compare_scored_candidates);

        let Some(scored_best) = scored.first() else {
            decisions.unmatched.push(UnmatchedRecord {
                target_id: target.target_id.clone(),
                reason: "no_candidates".to_string(),
                best_candidate: None,
            });
            continue;
        };

        let eligible = scored
            .iter()
            .filter(|candidate| !candidate.required_failed)
            .collect::<Vec<_>>();
        let Some(best) = eligible.first() else {
            decisions.unmatched.push(UnmatchedRecord {
                target_id: target.target_id.clone(),
                reason: "required_assertion_failed".to_string(),
                best_candidate: Some(scored_best.score.clone()),
            });
            continue;
        };

        let runner_up = eligible.get(1);
        let gap = runner_up.map(|candidate| score_gap(best.score.score, candidate.score.score));

        if best.score.score < strategy.match_threshold {
            decisions.unmatched.push(UnmatchedRecord {
                target_id: target.target_id.clone(),
                reason: "no_candidates_above_threshold".to_string(),
                best_candidate: Some(best.score.clone()),
            });
        } else if let Some(gap) = gap
            && gap < strategy.ambiguity_gap
        {
            decisions.ambiguous.push(AmbiguousRecord {
                target_id: target.target_id.clone(),
                candidates: ambiguous_candidates(
                    &eligible,
                    best.score.score,
                    strategy.ambiguity_gap,
                ),
                gap,
                reason: "insufficient_ambiguity_gap".to_string(),
            });
        } else {
            decisions.matches.push(MatchRecord {
                reference_id: best.score.reference_id.clone(),
                target_id: target.target_id.clone(),
                canonical_id: best.score.reference_id.clone(),
                score: best.score.score,
                assertions: best.score.assertions.clone(),
                runner_up: runner_up.map(|candidate| {
                    candidate
                        .score
                        .clone()
                        .with_gap(score_gap(best.score.score, candidate.score.score))
                }),
            });
        }
    }

    decisions.conflict_warnings = conflict_warnings(&decisions.matches);
    decisions
}

fn score_candidate_pair(
    reference_id: &str,
    reference: &super::ResolveRecord,
    target: &super::ResolveRecord,
    strategy: &ResolveStrategy,
    registry: Option<&Registry>,
) -> ScoredCandidate {
    let mut assertions = Vec::with_capacity(strategy.assertions.len());
    let mut score = 0.0;
    let mut required_failed = false;

    for assertion in &strategy.assertions {
        let result = evaluate_assertion(assertion, reference, target, registry);
        if result.passed {
            score += result.score * result.weight;
        } else if result.required {
            required_failed = true;
        }
        assertions.push(result);
    }

    ScoredCandidate {
        score: CandidateScore {
            reference_id: reference_id.to_string(),
            score,
            gap: None,
            assertions,
        },
        required_failed,
    }
}

fn compare_scored_candidates(left: &ScoredCandidate, right: &ScoredCandidate) -> Ordering {
    right
        .score
        .score
        .total_cmp(&left.score.score)
        .then_with(|| left.score.reference_id.cmp(&right.score.reference_id))
}

fn ambiguous_candidates(
    scored: &[&ScoredCandidate],
    best_score: f64,
    ambiguity_gap: f64,
) -> Vec<CandidateScore> {
    scored
        .iter()
        .take_while(|candidate| score_gap(best_score, candidate.score.score) < ambiguity_gap)
        .map(|candidate| {
            candidate
                .score
                .clone()
                .with_gap(score_gap(best_score, candidate.score.score))
        })
        .collect()
}

fn score_gap(best_score: f64, other_score: f64) -> f64 {
    best_score - other_score
}

fn conflict_warnings(matches: &[MatchRecord]) -> Vec<String> {
    let mut by_reference = BTreeMap::<String, Vec<String>>::new();
    for record in matches {
        by_reference
            .entry(record.reference_id.clone())
            .or_default()
            .push(record.target_id.clone());
    }

    by_reference
        .into_iter()
        .filter_map(|(reference_id, target_ids)| {
            if target_ids.len() > 1 {
                Some(format!(
                    "one_to_many_conflict: reference_id '{}' matched target_ids [{}]",
                    reference_id,
                    target_ids.join(", ")
                ))
            } else {
                None
            }
        })
        .collect()
}

impl CandidateScore {
    fn with_gap(mut self, gap: f64) -> Self {
        self.gap = Some(gap);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ScoredCandidate {
    score: CandidateScore,
    required_failed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InputFormat;
    use crate::resolve::{
        LoadedTape, LoadedTapes, ResolveIdentity, ResolveIdentitySide, ResolveOperatorSpec,
        TapeSide, select_candidates,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn strategy(assertions: Vec<ResolveOperatorSpec>) -> ResolveStrategy {
        ResolveStrategy {
            id: "score-test".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "loan".to_string(),
            identity: ResolveIdentity {
                reference: ResolveIdentitySide {
                    id_columns: vec!["loan_id".to_string()],
                },
                target: ResolveIdentitySide {
                    id_columns: vec!["target_id".to_string()],
                },
            },
            candidate_filter: vec![],
            assertions,
            match_threshold: 0.75,
            ambiguity_gap: 0.10,
            max_candidates: None,
            description: String::new(),
            content_hash: String::new(),
        }
    }

    fn assertion(
        op: &str,
        field_ref: &str,
        field_tgt: &str,
        weight: f64,
        required: bool,
        params: &[(&str, Value)],
    ) -> ResolveOperatorSpec {
        ResolveOperatorSpec {
            field_ref: field_ref.to_string(),
            field_tgt: field_tgt.to_string(),
            op: op.to_string(),
            weight,
            required,
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }

    fn standard_assertions(required_address: bool) -> Vec<ResolveOperatorSpec> {
        vec![
            assertion("exact", "address", "address", 0.60, required_address, &[]),
            assertion(
                "tolerance_pct",
                "upb",
                "balance",
                0.40,
                false,
                &[("tolerance", json!(0.05))],
            ),
        ]
    }

    fn record(side: TapeSide, id: &str, attrs: &[(&str, Value)]) -> crate::resolve::ResolveRecord {
        let id_key = match side {
            TapeSide::Reference => "loan_id",
            TapeSide::Target => "target_id",
        };
        let mut attributes = attrs
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        attributes.insert(id_key.to_string(), json!(id));

        crate::resolve::ResolveRecord {
            side,
            composite_id: id.to_string(),
            row_index: 0,
            attributes,
        }
    }

    fn loaded_tapes(
        reference: Vec<crate::resolve::ResolveRecord>,
        target: Vec<crate::resolve::ResolveRecord>,
    ) -> LoadedTapes {
        LoadedTapes {
            reference: LoadedTape {
                side: TapeSide::Reference,
                path: "reference.csv".to_string(),
                format: InputFormat::Csv,
                delimiter: Some(b','),
                records: reference,
            },
            target: LoadedTape {
                side: TapeSide::Target,
                path: "target.csv".to_string(),
                format: InputFormat::Csv,
                delimiter: Some(b','),
                records: target,
            },
        }
    }

    fn select(
        tapes: &LoadedTapes,
        strategy: &ResolveStrategy,
    ) -> crate::resolve::CandidateSelection {
        select_candidates(tapes, strategy, None, None).unwrap()
    }

    #[test]
    fn high_confidence_match_selects_best_candidate_with_runner_up_gap() {
        let strategy = strategy(standard_assertions(false));
        let tapes = loaded_tapes(
            vec![
                record(
                    TapeSide::Reference,
                    "R-1",
                    &[("address", json!("100 Main")), ("upb", json!(100))],
                ),
                record(
                    TapeSide::Reference,
                    "R-2",
                    &[("address", json!("Other")), ("upb", json!(102))],
                ),
            ],
            vec![record(
                TapeSide::Target,
                "T-1",
                &[("address", json!("100 Main")), ("balance", json!(102))],
            )],
        );

        let decisions = score_candidates(&select(&tapes, &strategy), &strategy, None);

        assert_eq!(decisions.matches.len(), 1);
        assert_eq!(decisions.matches[0].reference_id, "R-1");
        assert_eq!(decisions.matches[0].target_id, "T-1");
        assert_eq!(decisions.matches[0].canonical_id, "R-1");
        assert_eq!(decisions.matches[0].score, 1.0);
        let runner_up = decisions.matches[0].runner_up.as_ref().unwrap();
        assert_eq!(runner_up.reference_id, "R-2");
        assert_eq!(runner_up.score, 0.4);
        assert_eq!(runner_up.gap, Some(0.6));
    }

    #[test]
    fn below_threshold_candidate_is_unmatched_with_near_miss() {
        let strategy = strategy(standard_assertions(false));
        let tapes = loaded_tapes(
            vec![record(
                TapeSide::Reference,
                "R-1",
                &[("address", json!("Other")), ("upb", json!(100))],
            )],
            vec![record(
                TapeSide::Target,
                "T-1",
                &[("address", json!("100 Main")), ("balance", json!(102))],
            )],
        );

        let decisions = score_candidates(&select(&tapes, &strategy), &strategy, None);

        assert!(decisions.matches.is_empty());
        assert_eq!(decisions.unmatched.len(), 1);
        assert_eq!(
            decisions.unmatched[0].reason,
            "no_candidates_above_threshold"
        );
        let best = decisions.unmatched[0].best_candidate.as_ref().unwrap();
        assert_eq!(best.reference_id, "R-1");
        assert_eq!(best.score, 0.4);
        assert_eq!(best.assertions.len(), 2);
    }

    #[test]
    fn no_surviving_candidates_are_unmatched_without_near_miss() {
        let mut strategy = strategy(standard_assertions(false));
        strategy.candidate_filter = vec![assertion("exact", "deal", "deal", 0.0, false, &[])];
        let tapes = loaded_tapes(
            vec![record(
                TapeSide::Reference,
                "R-1",
                &[
                    ("deal", json!("D1")),
                    ("address", json!("100 Main")),
                    ("upb", json!(100)),
                ],
            )],
            vec![record(
                TapeSide::Target,
                "T-1",
                &[
                    ("deal", json!("D2")),
                    ("address", json!("100 Main")),
                    ("balance", json!(100)),
                ],
            )],
        );

        let decisions = score_candidates(&select(&tapes, &strategy), &strategy, None);

        assert_eq!(decisions.unmatched.len(), 1);
        assert_eq!(decisions.unmatched[0].reason, "no_candidates");
        assert!(decisions.unmatched[0].best_candidate.is_none());
    }

    #[test]
    fn insufficient_ambiguity_gap_yields_ambiguous_record() {
        let strategy = strategy(standard_assertions(false));
        let tapes = loaded_tapes(
            vec![
                record(
                    TapeSide::Reference,
                    "R-1",
                    &[("address", json!("100 Main")), ("upb", json!(100))],
                ),
                record(
                    TapeSide::Reference,
                    "R-2",
                    &[("address", json!("100 Main")), ("upb", json!(104))],
                ),
            ],
            vec![record(
                TapeSide::Target,
                "T-1",
                &[("address", json!("100 Main")), ("balance", json!(102))],
            )],
        );

        let decisions = score_candidates(&select(&tapes, &strategy), &strategy, None);

        assert!(decisions.matches.is_empty());
        assert_eq!(decisions.ambiguous.len(), 1);
        assert_eq!(decisions.ambiguous[0].target_id, "T-1");
        assert_eq!(decisions.ambiguous[0].gap, 0.0);
        assert_eq!(
            decisions.ambiguous[0]
                .candidates
                .iter()
                .map(|candidate| candidate.reference_id.as_str())
                .collect::<Vec<_>>(),
            vec!["R-1", "R-2"]
        );
    }

    #[test]
    fn exact_score_ties_use_lexicographic_reference_tie_break_when_gap_allows() {
        let mut strategy = strategy(standard_assertions(false));
        strategy.ambiguity_gap = 0.0;
        let tapes = loaded_tapes(
            vec![
                record(
                    TapeSide::Reference,
                    "R-2",
                    &[("address", json!("100 Main")), ("upb", json!(100))],
                ),
                record(
                    TapeSide::Reference,
                    "R-1",
                    &[("address", json!("100 Main")), ("upb", json!(100))],
                ),
            ],
            vec![record(
                TapeSide::Target,
                "T-1",
                &[("address", json!("100 Main")), ("balance", json!(100))],
            )],
        );

        let decisions = score_candidates(&select(&tapes, &strategy), &strategy, None);

        assert_eq!(decisions.matches.len(), 1);
        assert_eq!(decisions.matches[0].reference_id, "R-1");
        assert_eq!(
            decisions.matches[0]
                .runner_up
                .as_ref()
                .unwrap()
                .reference_id,
            "R-2"
        );
        assert_eq!(
            decisions.matches[0].runner_up.as_ref().unwrap().gap,
            Some(0.0)
        );
    }

    #[test]
    fn required_assertion_failure_blocks_match_even_above_threshold() {
        let strategy = strategy(vec![
            assertion("exact", "address", "address", 0.20, true, &[]),
            assertion(
                "tolerance_pct",
                "upb",
                "balance",
                0.80,
                false,
                &[("tolerance", json!(0.05))],
            ),
        ]);
        let tapes = loaded_tapes(
            vec![record(
                TapeSide::Reference,
                "R-1",
                &[("address", json!("Other")), ("upb", json!(100))],
            )],
            vec![record(
                TapeSide::Target,
                "T-1",
                &[("address", json!("100 Main")), ("balance", json!(100))],
            )],
        );

        let decisions = score_candidates(&select(&tapes, &strategy), &strategy, None);

        assert!(decisions.matches.is_empty());
        assert_eq!(decisions.unmatched[0].reason, "required_assertion_failed");
        let best = decisions.unmatched[0].best_candidate.as_ref().unwrap();
        assert_eq!(best.score, 0.8);
        assert!(best.assertions[0].required);
        assert!(!best.assertions[0].passed);
    }

    #[test]
    fn one_to_many_conflicts_are_reported_without_reassigning_matches() {
        let mut strategy = strategy(standard_assertions(false));
        strategy.ambiguity_gap = 0.0;
        let tapes = loaded_tapes(
            vec![
                record(
                    TapeSide::Reference,
                    "R-1",
                    &[("address", json!("100 Main")), ("upb", json!(100))],
                ),
                record(
                    TapeSide::Reference,
                    "R-2",
                    &[("address", json!("Other")), ("upb", json!(999))],
                ),
            ],
            vec![
                record(
                    TapeSide::Target,
                    "T-1",
                    &[("address", json!("100 Main")), ("balance", json!(100))],
                ),
                record(
                    TapeSide::Target,
                    "T-2",
                    &[("address", json!("100 Main")), ("balance", json!(100))],
                ),
            ],
        );

        let decisions = score_candidates(&select(&tapes, &strategy), &strategy, None);

        assert_eq!(decisions.matches.len(), 2);
        assert_eq!(
            decisions.conflict_warnings,
            vec!["one_to_many_conflict: reference_id 'R-1' matched target_ids [T-1, T-2]"]
        );
    }

    #[test]
    fn output_ordering_is_stable_across_repeated_runs() {
        let strategy = strategy(standard_assertions(false));
        let tapes = loaded_tapes(
            vec![
                record(
                    TapeSide::Reference,
                    "R-2",
                    &[("address", json!("Other")), ("upb", json!(100))],
                ),
                record(
                    TapeSide::Reference,
                    "R-1",
                    &[("address", json!("100 Main")), ("upb", json!(100))],
                ),
            ],
            vec![
                record(
                    TapeSide::Target,
                    "T-2",
                    &[("address", json!("Other")), ("balance", json!(100))],
                ),
                record(
                    TapeSide::Target,
                    "T-1",
                    &[("address", json!("100 Main")), ("balance", json!(100))],
                ),
            ],
        );

        let first = score_candidates(&select(&tapes, &strategy), &strategy, None);
        let second = score_candidates(&select(&tapes, &strategy), &strategy, None);

        assert_eq!(first, second);
        assert_eq!(
            first
                .matches
                .iter()
                .map(|record| record.target_id.as_str())
                .collect::<Vec<_>>(),
            vec!["T-1", "T-2"]
        );
    }
}
