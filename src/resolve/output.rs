use super::{
    AmbiguousRecord, CANON_RESOLVE_VERSION, CandidateScore, GoldScore, LoadedTapes, MatchDecisions,
    MatchRecord, ResolveArtifact, ResolveRegistrySnapshot, ResolveStrategy, ResolveSummary,
    UnmatchedRecord, WriteBackSummary,
};
use crate::Registry;

pub fn build_artifact(
    strategy: &ResolveStrategy,
    registry: &Registry,
    tapes: &LoadedTapes,
    decisions: MatchDecisions,
    gold_score: Option<GoldScore>,
    write_back: Option<WriteBackSummary>,
) -> ResolveArtifact {
    let mut matches = decisions.matches;
    let mut unmatched = decisions.unmatched;
    let mut ambiguous = decisions.ambiguous;
    let mut conflict_warnings = decisions.conflict_warnings;

    normalize_ordering(
        &mut matches,
        &mut unmatched,
        &mut ambiguous,
        &mut conflict_warnings,
    );

    let summary = build_summary(tapes.target.records.len(), &matches, &unmatched, &ambiguous);
    debug_assert!(summary.partition_holds());

    ResolveArtifact {
        version: CANON_RESOLVE_VERSION.to_string(),
        strategy: strategy.reference(),
        registry: ResolveRegistrySnapshot {
            id: registry.meta.id.clone(),
            version: registry.meta.version.clone(),
            source: registry.meta.source.clone(),
        },
        reference_tape: tapes.reference.summary(),
        target_tape: tapes.target.summary(),
        summary,
        matches,
        unmatched,
        ambiguous,
        conflict_warnings,
        gold_score,
        write_back,
    }
}

pub fn build_summary(
    target_records: usize,
    matches: &[MatchRecord],
    unmatched: &[UnmatchedRecord],
    ambiguous: &[AmbiguousRecord],
) -> ResolveSummary {
    let matched = matches.len();
    let unmatched_count = unmatched.len();
    let ambiguous_count = ambiguous.len();
    let match_rate = if target_records == 0 {
        0.0
    } else {
        matched as f64 / target_records as f64
    };

    ResolveSummary {
        target_records,
        matched,
        unmatched: unmatched_count,
        ambiguous: ambiguous_count,
        match_rate,
    }
}

fn normalize_ordering(
    matches: &mut [MatchRecord],
    unmatched: &mut [UnmatchedRecord],
    ambiguous: &mut [AmbiguousRecord],
    conflict_warnings: &mut [String],
) {
    matches.sort_by(|left, right| {
        left.target_id
            .cmp(&right.target_id)
            .then(left.reference_id.cmp(&right.reference_id))
    });
    unmatched.sort_by(|left, right| left.target_id.cmp(&right.target_id));
    ambiguous.sort_by(|left, right| left.target_id.cmp(&right.target_id));
    for record in ambiguous {
        sort_candidate_scores(&mut record.candidates);
    }
    conflict_warnings.sort();
}

fn sort_candidate_scores(candidates: &mut [CandidateScore]) {
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then(left.reference_id.cmp(&right.reference_id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::load_registry;
    use crate::resolve::{
        AssertionResult, CandidateSelection, ResolveIdentity, ResolveIdentitySide,
        ResolveOperatorSpec, TapeLoadOptions, load_strategy, load_tapes, score_candidates,
        select_candidates,
    };
    use crate::{InputFormat, RegistryMeta};
    use serde_json::{Value, json};
    use std::{collections::BTreeMap, path::Path, path::PathBuf};

    fn strategy() -> ResolveStrategy {
        ResolveStrategy {
            id: "output-test".to_string(),
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
            assertions: vec![ResolveOperatorSpec {
                field_ref: "address".to_string(),
                field_tgt: "address".to_string(),
                op: "exact".to_string(),
                weight: 1.0,
                required: false,
                params: BTreeMap::new(),
            }],
            match_threshold: 0.75,
            ambiguity_gap: 0.10,
            max_candidates: None,
            description: String::new(),
            content_hash: "blake3:test".to_string(),
        }
    }

    fn registry() -> Registry {
        Registry {
            meta: RegistryMeta {
                id: "resolve-output".to_string(),
                version: "1.0.0".to_string(),
                source: "tests/registry".to_string(),
            },
            db_path: PathBuf::from("unused.sqlite"),
        }
    }

    fn empty_tapes() -> LoadedTapes {
        LoadedTapes {
            reference: crate::resolve::LoadedTape {
                side: crate::resolve::TapeSide::Reference,
                path: "reference.csv".to_string(),
                format: InputFormat::Csv,
                delimiter: Some(b','),
                records: vec![],
            },
            target: crate::resolve::LoadedTape {
                side: crate::resolve::TapeSide::Target,
                path: "target.csv".to_string(),
                format: InputFormat::Csv,
                delimiter: Some(b','),
                records: vec![crate::resolve::ResolveRecord {
                    side: crate::resolve::TapeSide::Target,
                    composite_id: "T-1".to_string(),
                    row_index: 0,
                    attributes: BTreeMap::new(),
                }],
            },
        }
    }

    fn assertion_result(field_ref: &str, field_tgt: &str, passed: bool) -> AssertionResult {
        AssertionResult {
            field_ref: field_ref.to_string(),
            field_tgt: field_tgt.to_string(),
            op: "exact".to_string(),
            passed,
            score: if passed { 1.0 } else { 0.0 },
            weight: 1.0,
            required: false,
            detail: BTreeMap::new(),
        }
    }

    fn candidate(reference_id: &str, score: f64) -> CandidateScore {
        CandidateScore {
            reference_id: reference_id.to_string(),
            score,
            gap: None,
            assertions: vec![assertion_result("address", "address", score > 0.0)],
        }
    }

    #[test]
    fn build_artifact_populates_contract_shape_and_summary_invariant() {
        let decisions = MatchDecisions {
            matches: vec![MatchRecord {
                reference_id: "R-2".to_string(),
                target_id: "T-2".to_string(),
                canonical_id: "R-2".to_string(),
                score: 1.0,
                assertions: vec![assertion_result("address", "address", true)],
                runner_up: Some(candidate("R-1", 0.25)),
            }],
            unmatched: vec![UnmatchedRecord {
                target_id: "T-1".to_string(),
                reason: "no_candidates_above_threshold".to_string(),
                best_candidate: Some(candidate("R-3", 0.4)),
            }],
            ambiguous: vec![AmbiguousRecord {
                target_id: "T-3".to_string(),
                candidates: vec![candidate("R-9", 0.81), candidate("R-8", 0.81)],
                gap: 0.0,
                reason: "insufficient_ambiguity_gap".to_string(),
            }],
            conflict_warnings: vec!["z warning".to_string(), "a warning".to_string()],
        };
        let mut tapes = empty_tapes();
        tapes.target.records.push(crate::resolve::ResolveRecord {
            side: crate::resolve::TapeSide::Target,
            composite_id: "T-2".to_string(),
            row_index: 1,
            attributes: BTreeMap::new(),
        });
        tapes.target.records.push(crate::resolve::ResolveRecord {
            side: crate::resolve::TapeSide::Target,
            composite_id: "T-3".to_string(),
            row_index: 2,
            attributes: BTreeMap::new(),
        });

        let artifact = build_artifact(&strategy(), &registry(), &tapes, decisions, None, None);

        assert_eq!(artifact.version, "canon_resolve.v0");
        assert_eq!(artifact.strategy.content_hash, "blake3:test");
        assert_eq!(artifact.registry.id, "resolve-output");
        assert_eq!(artifact.reference_tape.record_count, 0);
        assert_eq!(artifact.target_tape.record_count, 3);
        assert!(artifact.summary.partition_holds());
        assert_eq!(artifact.summary.target_records, 3);
        assert_eq!(artifact.summary.matched, 1);
        assert_eq!(artifact.summary.unmatched, 1);
        assert_eq!(artifact.summary.ambiguous, 1);
        assert_eq!(artifact.summary.match_rate, 1.0 / 3.0);
        assert_eq!(artifact.conflict_warnings, vec!["a warning", "z warning"]);
    }

    #[test]
    fn build_artifact_normalizes_record_and_candidate_ordering() {
        let decisions = MatchDecisions {
            matches: vec![
                MatchRecord {
                    reference_id: "R-2".to_string(),
                    target_id: "T-2".to_string(),
                    canonical_id: "R-2".to_string(),
                    score: 1.0,
                    assertions: vec![],
                    runner_up: None,
                },
                MatchRecord {
                    reference_id: "R-1".to_string(),
                    target_id: "T-1".to_string(),
                    canonical_id: "R-1".to_string(),
                    score: 1.0,
                    assertions: vec![],
                    runner_up: None,
                },
            ],
            unmatched: vec![
                UnmatchedRecord {
                    target_id: "T-4".to_string(),
                    reason: "no_candidates".to_string(),
                    best_candidate: None,
                },
                UnmatchedRecord {
                    target_id: "T-3".to_string(),
                    reason: "no_candidates".to_string(),
                    best_candidate: None,
                },
            ],
            ambiguous: vec![AmbiguousRecord {
                target_id: "T-5".to_string(),
                candidates: vec![candidate("R-10", 0.8), candidate("R-9", 0.9)],
                gap: 0.1,
                reason: "insufficient_ambiguity_gap".to_string(),
            }],
            conflict_warnings: vec![],
        };
        let mut tapes = empty_tapes();
        for target_id in ["T-2", "T-3", "T-4", "T-5"] {
            tapes.target.records.push(crate::resolve::ResolveRecord {
                side: crate::resolve::TapeSide::Target,
                composite_id: target_id.to_string(),
                row_index: 0,
                attributes: BTreeMap::new(),
            });
        }

        let artifact = build_artifact(&strategy(), &registry(), &tapes, decisions, None, None);

        assert_eq!(
            artifact
                .matches
                .iter()
                .map(|record| record.target_id.as_str())
                .collect::<Vec<_>>(),
            vec!["T-1", "T-2"]
        );
        assert_eq!(
            artifact
                .unmatched
                .iter()
                .map(|record| record.target_id.as_str())
                .collect::<Vec<_>>(),
            vec!["T-3", "T-4"]
        );
        assert_eq!(
            artifact.ambiguous[0]
                .candidates
                .iter()
                .map(|candidate| candidate.reference_id.as_str())
                .collect::<Vec<_>>(),
            vec!["R-9", "R-10"]
        );
    }

    #[test]
    fn summary_rendering_is_concise_and_operator_friendly() {
        let artifact = ResolveArtifact {
            strategy: strategy().reference(),
            registry: ResolveRegistrySnapshot {
                id: "resolve-output".to_string(),
                version: "1.0.0".to_string(),
                source: "registry".to_string(),
            },
            summary: ResolveSummary {
                target_records: 3,
                matched: 1,
                unmatched: 1,
                ambiguous: 1,
                match_rate: 1.0 / 3.0,
            },
            conflict_warnings: vec!["conflict".to_string()],
            ..ResolveArtifact::default()
        };

        assert_eq!(
            artifact.render_summary(),
            "canon_resolve.v0 strategy=output-test registry=resolve-output@1.0.0 target_records=3 matched=1 unmatched=1 ambiguous=1 match_rate=0.333 conflicts=1"
        );
    }

    #[test]
    fn json_serialization_is_deterministic_and_excludes_attribute_store() {
        let decisions = MatchDecisions {
            matches: vec![MatchRecord {
                reference_id: "R-1".to_string(),
                target_id: "T-1".to_string(),
                canonical_id: "R-1".to_string(),
                score: 1.0,
                assertions: vec![assertion_result("address", "address", true)],
                runner_up: None,
            }],
            ..MatchDecisions::default()
        };
        let artifact = build_artifact(
            &strategy(),
            &registry(),
            &empty_tapes(),
            decisions,
            None,
            None,
        );

        let first = serde_json::to_string(&artifact).unwrap();
        let second = serde_json::to_string(&artifact).unwrap();
        assert_eq!(first, second);
        assert!(!first.contains("attributes"));

        let value: Value = serde_json::from_str(&first).unwrap();
        assert_eq!(value["version"], json!("canon_resolve.v0"));
        assert_eq!(
            value["matches"][0]["assertions"][0]["field_ref"],
            json!("address")
        );
    }

    #[test]
    fn fixture_corpus_output_is_byte_stable_for_same_inputs() {
        let strategy = load_strategy(Path::new(
            "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
        ))
        .expect("load strategy fixture");
        let tapes = load_tapes(
            Path::new("tests/fixtures/resolve/tapes/reference_loans.csv"),
            Path::new("tests/fixtures/resolve/tapes/target_loans.csv"),
            &strategy,
            TapeLoadOptions {
                max_rows: None,
                max_bytes: None,
            },
        )
        .expect("load tape fixtures");
        let registry = load_registry(Path::new("tests/fixtures/registries/resolve-servicers"))
            .expect("load registry fixture");

        let artifact = fixture_artifact(&strategy, &registry, &tapes);
        let repeated = fixture_artifact(&strategy, &registry, &tapes);

        assert!(artifact.summary.partition_holds());
        assert_eq!(artifact.reference_tape.record_count, 10);
        assert_eq!(artifact.target_tape.record_count, 12);
        assert_eq!(
            serde_json::to_string(&artifact).unwrap(),
            serde_json::to_string(&repeated).unwrap()
        );
    }

    fn fixture_artifact(
        strategy: &ResolveStrategy,
        registry: &Registry,
        tapes: &LoadedTapes,
    ) -> ResolveArtifact {
        let selection: CandidateSelection =
            select_candidates(tapes, strategy, Some(registry), None).expect("select candidates");
        let decisions = score_candidates(&selection, strategy, Some(registry));
        build_artifact(strategy, registry, tapes, decisions, None, None)
    }
}
