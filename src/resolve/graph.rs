use super::{
    AssertionResult, LoadedTapes, ResolveError, ResolveErrorCode, ResolveOperatorSpec,
    ResolveRecord, ResolveResult, ResolveStrategy, evaluate_assertion,
};
use crate::Registry;
use petgraph::graph::{Graph, NodeIndex};
use serde_json::json;

pub type ResolvePetGraph = Graph<ResolveNode, ResolveEdge>;

#[derive(Debug, Clone, PartialEq)]
pub struct ResolveNode {
    pub record: ResolveRecord,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolveEdge {
    pub score: f64,
    pub assertions: Vec<AssertionResult>,
}

#[derive(Debug, Clone)]
pub struct ResolveGraph {
    pub graph: ResolvePetGraph,
    pub reference_nodes: Vec<NodeIndex>,
    pub target_nodes: Vec<NodeIndex>,
}

impl ResolveGraph {
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn record(&self, index: NodeIndex) -> &ResolveRecord {
        &self.graph[index].record
    }

    pub fn reference_records(&self) -> impl Iterator<Item = &ResolveRecord> {
        self.reference_nodes.iter().map(|index| self.record(*index))
    }

    pub fn target_records(&self) -> impl Iterator<Item = &ResolveRecord> {
        self.target_nodes.iter().map(|index| self.record(*index))
    }
}

#[derive(Debug, Clone)]
pub struct CandidateSelection {
    pub graph: ResolveGraph,
    pub targets: Vec<TargetCandidates>,
}

impl CandidateSelection {
    pub fn total_candidate_pairs(&self) -> usize {
        self.targets
            .iter()
            .map(|target| target.candidates.len())
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetCandidates {
    pub target_id: String,
    pub target_node: NodeIndex,
    pub candidates: Vec<CandidatePair>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidatePair {
    pub reference_id: String,
    pub reference_node: NodeIndex,
    pub filters: Vec<AssertionResult>,
}

pub fn hydrate_graph(tapes: &LoadedTapes) -> ResolveGraph {
    let mut graph = Graph::new();

    let reference_nodes = tapes
        .reference
        .records_sorted_by_id()
        .into_iter()
        .map(|record| add_record_node(&mut graph, record))
        .collect::<Vec<_>>();

    let target_nodes = tapes
        .target
        .records_sorted_by_id()
        .into_iter()
        .map(|record| add_record_node(&mut graph, record))
        .collect::<Vec<_>>();

    ResolveGraph {
        graph,
        reference_nodes,
        target_nodes,
    }
}

pub fn select_candidates(
    tapes: &LoadedTapes,
    strategy: &ResolveStrategy,
    registry: Option<&Registry>,
    cli_max_candidates: Option<usize>,
) -> ResolveResult<CandidateSelection> {
    let graph = hydrate_graph(tapes);
    let max_candidates = cli_max_candidates.or(strategy.max_candidates);
    let mut targets = Vec::with_capacity(graph.target_nodes.len());

    for target_node in &graph.target_nodes {
        let target = graph.record(*target_node);
        let mut candidates = graph
            .reference_nodes
            .iter()
            .map(|reference_node| CandidatePair {
                reference_id: graph.record(*reference_node).composite_id.clone(),
                reference_node: *reference_node,
                filters: Vec::with_capacity(strategy.candidate_filter.len()),
            })
            .collect::<Vec<_>>();

        for filter in &strategy.candidate_filter {
            candidates = apply_filter(&graph, target, candidates, filter, registry);
            if candidates.is_empty() {
                break;
            }
        }

        if let Some(limit) = max_candidates
            && candidates.len() > limit
        {
            return Err(ResolveError::with_detail(
                ResolveErrorCode::TooManyCandidates,
                format!(
                    "Target '{}' has {} candidates after filtering, above limit {}",
                    target.composite_id,
                    candidates.len(),
                    limit
                ),
                json!({
                    "target_id": target.composite_id,
                    "candidate_count": candidates.len(),
                    "max_candidates": limit,
                    "filter_count": strategy.candidate_filter.len()
                }),
            ));
        }

        targets.push(TargetCandidates {
            target_id: target.composite_id.clone(),
            target_node: *target_node,
            candidates,
        });
    }

    Ok(CandidateSelection { graph, targets })
}

fn add_record_node(graph: &mut ResolvePetGraph, record: &ResolveRecord) -> NodeIndex {
    graph.add_node(ResolveNode {
        record: record.clone(),
    })
}

fn apply_filter(
    graph: &ResolveGraph,
    target: &ResolveRecord,
    candidates: Vec<CandidatePair>,
    filter: &ResolveOperatorSpec,
    registry: Option<&Registry>,
) -> Vec<CandidatePair> {
    candidates
        .into_iter()
        .filter_map(|mut candidate| {
            let reference = graph.record(candidate.reference_node);
            let result = evaluate_assertion(filter, reference, target, registry);
            if result.passed {
                candidate.filters.push(result);
                Some(candidate)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::load_registry;
    use crate::resolve::{LoadedTape, ResolveIdentity, ResolveIdentitySide, TapeSide};
    use crate::{InputFormat, RegistryMeta};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::path::Path;

    fn strategy(filters: Vec<ResolveOperatorSpec>) -> ResolveStrategy {
        ResolveStrategy {
            id: "test-strategy".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "loan".to_string(),
            identity: ResolveIdentity {
                reference: ResolveIdentitySide {
                    id_columns: vec!["loan_id".to_string()],
                },
                target: ResolveIdentitySide {
                    id_columns: vec!["deal".to_string(), "loan_number".to_string()],
                },
            },
            candidate_filter: filters,
            assertions: vec![ResolveOperatorSpec {
                field_ref: "loan_id".to_string(),
                field_tgt: "loan_number".to_string(),
                op: "prefix".to_string(),
                weight: 1.0,
                required: false,
                params: BTreeMap::new(),
            }],
            match_threshold: 0.75,
            ambiguity_gap: 0.15,
            max_candidates: None,
            description: String::new(),
            content_hash: String::new(),
        }
    }

    fn filter(
        op: &str,
        field_ref: &str,
        field_tgt: &str,
        params: &[(&str, Value)],
    ) -> ResolveOperatorSpec {
        ResolveOperatorSpec {
            field_ref: field_ref.to_string(),
            field_tgt: field_tgt.to_string(),
            op: op.to_string(),
            weight: 0.0,
            required: false,
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }

    fn record(side: TapeSide, id: &str, attrs: &[(&str, Value)]) -> ResolveRecord {
        ResolveRecord {
            side,
            composite_id: id.to_string(),
            row_index: 0,
            attributes: attrs
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }

    fn tape(side: TapeSide, records: Vec<ResolveRecord>) -> LoadedTape {
        LoadedTape {
            side,
            path: format!("{side:?}.csv"),
            format: InputFormat::Csv,
            delimiter: Some(b','),
            records,
        }
    }

    fn tapes(reference: Vec<ResolveRecord>, target: Vec<ResolveRecord>) -> LoadedTapes {
        LoadedTapes {
            reference: tape(TapeSide::Reference, reference),
            target: tape(TapeSide::Target, target),
        }
    }

    fn simple_tapes() -> LoadedTapes {
        tapes(
            vec![
                record(
                    TapeSide::Reference,
                    "R-2",
                    &[
                        ("deal", json!("D1")),
                        ("upb", json!(120)),
                        ("servicer", json!("JPMorgan")),
                    ],
                ),
                record(
                    TapeSide::Reference,
                    "R-1",
                    &[
                        ("deal", json!("D1")),
                        ("upb", json!(100)),
                        ("servicer", json!("Wells Fargo")),
                    ],
                ),
                record(
                    TapeSide::Reference,
                    "R-3",
                    &[
                        ("deal", json!("D2")),
                        ("upb", json!(100)),
                        ("servicer", json!("KeyBank")),
                    ],
                ),
            ],
            vec![
                record(
                    TapeSide::Target,
                    "T-2",
                    &[
                        ("deal", json!("D2")),
                        ("balance", json!(102)),
                        ("servicer_name", json!("KeyBank")),
                    ],
                ),
                record(
                    TapeSide::Target,
                    "T-1",
                    &[
                        ("deal", json!("D1")),
                        ("balance", json!(103)),
                        ("servicer_name", json!("Wells Fargo Bank N.A.")),
                    ],
                ),
            ],
        )
    }

    #[test]
    fn hydrate_graph_inserts_sorted_reference_then_target_nodes() {
        let tapes = simple_tapes();
        let graph = hydrate_graph(&tapes);

        assert_eq!(graph.node_count(), 5);
        assert_eq!(graph.edge_count(), 0);

        let reference_ids = graph
            .reference_records()
            .map(|record| record.composite_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(reference_ids, vec!["R-1", "R-2", "R-3"]);
        assert!(
            graph
                .reference_records()
                .all(|record| record.side == TapeSide::Reference)
        );

        let target_ids = graph
            .target_records()
            .map(|record| record.composite_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(target_ids, vec!["T-1", "T-2"]);
        assert!(
            graph
                .target_records()
                .all(|record| record.side == TapeSide::Target)
        );
    }

    #[test]
    fn no_filters_emit_full_cross_product_in_deterministic_order() {
        let tapes = simple_tapes();
        let selection = select_candidates(&tapes, &strategy(vec![]), None, None).unwrap();

        assert_eq!(selection.total_candidate_pairs(), 6);
        assert_eq!(
            selection
                .targets
                .iter()
                .map(|target| target.target_id.as_str())
                .collect::<Vec<_>>(),
            vec!["T-1", "T-2"]
        );
        for target in &selection.targets {
            assert_eq!(
                target
                    .candidates
                    .iter()
                    .map(|candidate| candidate.reference_id.as_str())
                    .collect::<Vec<_>>(),
                vec!["R-1", "R-2", "R-3"]
            );
            assert!(
                target
                    .candidates
                    .iter()
                    .all(|candidate| candidate.filters.is_empty())
            );
        }
    }

    #[test]
    fn exact_and_range_filters_narrow_candidates_in_declaration_order() {
        let filters = vec![
            filter("exact", "deal", "deal", &[]),
            filter("range", "upb", "balance", &[("range_pct", json!(0.05))]),
        ];
        let selection = select_candidates(&simple_tapes(), &strategy(filters), None, None).unwrap();

        let target_one = selection
            .targets
            .iter()
            .find(|target| target.target_id == "T-1")
            .unwrap();
        assert_eq!(target_one.candidates.len(), 1);
        assert_eq!(target_one.candidates[0].reference_id, "R-1");
        assert_eq!(target_one.candidates[0].filters.len(), 2);
        assert_eq!(target_one.candidates[0].filters[0].op, "exact");
        assert_eq!(target_one.candidates[0].filters[1].op, "range");

        let target_two = selection
            .targets
            .iter()
            .find(|target| target.target_id == "T-2")
            .unwrap();
        assert_eq!(target_two.candidates.len(), 1);
        assert_eq!(target_two.candidates[0].reference_id, "R-3");
    }

    #[test]
    fn canon_match_filter_uses_loaded_registry_context() {
        let registry = load_registry(Path::new("tests/fixtures/registries/resolve-servicers"))
            .expect("load registry fixture");
        let filters = vec![filter("canon_match", "servicer", "servicer_name", &[])];
        let selection =
            select_candidates(&simple_tapes(), &strategy(filters), Some(&registry), None).unwrap();

        let target_one = selection
            .targets
            .iter()
            .find(|target| target.target_id == "T-1")
            .unwrap();
        assert_eq!(target_one.candidates.len(), 1);
        assert_eq!(target_one.candidates[0].reference_id, "R-1");
        assert_eq!(
            target_one.candidates[0].filters[0]
                .detail
                .get("ref_canonical_id"),
            Some(&json!("SERVICER-WELLS-FARGO"))
        );
    }

    #[test]
    fn no_candidate_behavior_is_explicit_empty_candidate_list() {
        let filters = vec![filter("exact", "deal", "missing_target_deal", &[])];
        let selection = select_candidates(&simple_tapes(), &strategy(filters), None, None).unwrap();

        assert_eq!(selection.targets.len(), 2);
        assert!(
            selection
                .targets
                .iter()
                .all(|target| target.candidates.is_empty())
        );
        assert_eq!(selection.total_candidate_pairs(), 0);
    }

    #[test]
    fn max_candidates_limit_refuses_per_target_after_filtering() {
        let filters = vec![filter("exact", "deal", "deal", &[])];
        let error = select_candidates(&simple_tapes(), &strategy(filters), None, Some(1))
            .expect_err("T-1 has two D1 candidates");

        assert_eq!(error.code, ResolveErrorCode::TooManyCandidates);
        let detail = error.detail.unwrap();
        assert_eq!(detail.get("target_id"), Some(&json!("T-1")));
        assert_eq!(detail.get("candidate_count"), Some(&json!(2)));
        assert_eq!(detail.get("max_candidates"), Some(&json!(1)));
    }

    #[test]
    fn strategy_max_candidates_applies_when_cli_limit_is_absent() {
        let mut limited_strategy = strategy(vec![filter("exact", "deal", "deal", &[])]);
        limited_strategy.max_candidates = Some(1);
        let error = select_candidates(&simple_tapes(), &limited_strategy, None, None)
            .expect_err("strategy max_candidates should apply");

        assert_eq!(error.code, ResolveErrorCode::TooManyCandidates);
    }

    #[test]
    fn cli_max_candidates_overrides_strategy_limit() {
        let mut limited_strategy = strategy(vec![filter("exact", "deal", "deal", &[])]);
        limited_strategy.max_candidates = Some(1);
        let selection = select_candidates(&simple_tapes(), &limited_strategy, None, Some(3))
            .expect("CLI limit should override strategy limit");

        assert_eq!(selection.total_candidate_pairs(), 3);
    }

    #[test]
    fn fixture_filters_avoid_full_output_cross_product() {
        let strategy = crate::resolve::load_strategy(Path::new(
            "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
        ))
        .expect("load strategy fixture");
        let tapes = crate::resolve::load_tapes(
            Path::new("tests/fixtures/resolve/tapes/reference_loans.csv"),
            Path::new("tests/fixtures/resolve/tapes/target_loans.csv"),
            &strategy,
            crate::resolve::TapeLoadOptions {
                max_rows: None,
                max_bytes: None,
            },
        )
        .expect("load tape fixtures");
        let registry = load_registry(Path::new("tests/fixtures/registries/resolve-servicers"))
            .expect("load registry fixture");
        let full_cross_product = tapes.reference.records.len() * tapes.target.records.len();

        let selection = select_candidates(&tapes, &strategy, Some(&registry), None)
            .expect("fixture selection should stay under max_candidates");

        assert!(selection.total_candidate_pairs() < full_cross_product);
        assert!(
            selection
                .targets
                .iter()
                .any(|target| !target.candidates.is_empty())
        );
    }

    #[test]
    fn graph_types_do_not_require_registry_persistence() {
        let graph = hydrate_graph(&simple_tapes());
        assert!(graph.graph.externals(petgraph::Direction::Outgoing).count() > 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn candidate_graph_records_are_retrievable_by_node_index() {
        let selection = select_candidates(&simple_tapes(), &strategy(vec![]), None, None).unwrap();
        let first_target = &selection.targets[0];
        let first_candidate = &first_target.candidates[0];

        assert_eq!(
            selection.graph.record(first_target.target_node).side,
            TapeSide::Target
        );
        assert_eq!(
            selection.graph.record(first_candidate.reference_node).side,
            TapeSide::Reference
        );
    }

    #[test]
    fn graph_selection_has_no_dependency_on_registry_metadata_for_non_canon_filters() {
        let registry = Registry {
            meta: RegistryMeta {
                id: "unused".to_string(),
                version: "0.0.0".to_string(),
                source: "unused".to_string(),
            },
            db_path: Path::new("does-not-exist.sqlite").to_path_buf(),
        };
        let filters = vec![filter("exact", "deal", "deal", &[])];
        let selection =
            select_candidates(&simple_tapes(), &strategy(filters), Some(&registry), None)
                .expect("exact filter should not touch registry");

        assert_eq!(selection.total_candidate_pairs(), 3);
    }
}
