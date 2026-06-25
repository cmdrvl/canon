//! Evidence-edge generation for `canon entity`.

use super::types::{
    BlockRecord, EdgeRecord, EntityError, EntityErrorCode, EntityResult, EntityStrategy,
    EvidenceHit, EvidenceKind, IncumbentMemory, ProjectedObservation, RegistrySnapshot,
    StrategyOperator, StrategyReference,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const NAME_NAMESPACE: &str = "name";
const CONTEXT_NAMESPACE: &str = "context";
const ANCHOR_NAMESPACE: &str = "anchor";
const REGISTRY_NAMESPACE: &str = "registry";

const STOPWORDS: &[&str] = &["a", "an", "and", "for", "in", "of", "the", "to"];
const LEGAL_SUFFIXES: &[&str] = &[
    "co",
    "company",
    "corp",
    "corporation",
    "inc",
    "incorporated",
    "limited",
    "llc",
    "lp",
    "ltd",
    "plc",
];

pub fn build_edges(
    strategy: &EntityStrategy,
    observations: &[ProjectedObservation],
    candidates: &[BlockRecord],
    incumbent: &IncumbentMemory,
) -> EntityResult<Vec<EdgeRecord>> {
    let facts = ObservationFactsIndex::build(strategy, observations, incumbent)?;
    let strategy_reference = strategy.reference();
    let mut ordered_candidates = candidates.to_vec();
    ordered_candidates.sort_by(|left, right| {
        (&left.left_row_id, &left.right_row_id).cmp(&(&right.left_row_id, &right.right_row_id))
    });

    ordered_candidates
        .iter()
        .map(|candidate| build_edge_record(&strategy_reference, &facts, candidate))
        .collect()
}

fn build_edge_record(
    strategy_reference: &StrategyReference,
    facts: &ObservationFactsIndex,
    candidate: &BlockRecord,
) -> EntityResult<EdgeRecord> {
    validate_candidate(candidate, strategy_reference, &facts.registry_snapshot)?;

    let left = facts
        .observations
        .get(&candidate.left_row_id)
        .ok_or_else(|| missing_row_error("left_row_id", &candidate.left_row_id))?;
    let right = facts
        .observations
        .get(&candidate.right_row_id)
        .ok_or_else(|| missing_row_error("right_row_id", &candidate.right_row_id))?;

    let mut hits = Vec::new();

    for operator in &facts.strategy.evidence.must_link {
        if let Some(hit) = evaluate_must_link(operator, left, right)? {
            hits.push(hit);
        }
    }
    for operator in &facts.strategy.evidence.support {
        if let Some(hit) = evaluate_support(operator, left, right)? {
            hits.push(hit);
        }
    }
    for operator in &facts.strategy.evidence.cannot_link {
        if let Some(hit) = evaluate_cannot_link(operator, left, right)? {
            hits.push(hit);
        }
    }

    let mut pair_score_by_namespace = BTreeMap::new();
    for hit in &hits {
        if matches!(hit.kind, EvidenceKind::CannotLink) {
            continue;
        }

        pair_score_by_namespace
            .entry(hit.namespace.clone())
            .and_modify(|current: &mut i64| *current = (*current).max(hit.score))
            .or_insert(hit.score);
    }

    let pair_score_total = pair_score_by_namespace.values().sum();
    let has_must_link = hits
        .iter()
        .any(|hit| matches!(hit.kind, EvidenceKind::MustLink));
    let has_cannot_link = hits
        .iter()
        .any(|hit| matches!(hit.kind, EvidenceKind::CannotLink));

    Ok(EdgeRecord {
        version: super::types::CANON_ENTITY_EDGE_VERSION.to_string(),
        strategy: strategy_reference.clone(),
        registry_snapshot: facts.registry_snapshot.clone(),
        left_row_id: candidate.left_row_id.clone(),
        right_row_id: candidate.right_row_id.clone(),
        hits,
        pair_score_by_namespace,
        pair_score_total,
        has_must_link,
        has_cannot_link,
    })
}

fn validate_candidate(
    candidate: &BlockRecord,
    strategy_reference: &StrategyReference,
    registry_snapshot: &RegistrySnapshot,
) -> EntityResult<()> {
    if candidate.version != super::types::CANON_ENTITY_BLOCK_VERSION {
        return Err(artifact_error(
            "Candidate block artifact version does not match canon_entity_block.v0",
            json!({
                "expected": super::types::CANON_ENTITY_BLOCK_VERSION,
                "actual": candidate.version,
            }),
        ));
    }

    if candidate.strategy != *strategy_reference {
        return Err(artifact_error(
            "Candidate block artifact strategy does not match the active strategy",
            json!({
                "expected": strategy_reference,
                "actual": candidate.strategy,
            }),
        ));
    }

    if candidate.registry_snapshot != *registry_snapshot {
        return Err(artifact_error(
            "Candidate block artifact registry snapshot does not match the active registry snapshot",
            json!({
                "expected": registry_snapshot,
                "actual": candidate.registry_snapshot,
            }),
        ));
    }

    Ok(())
}

fn evaluate_must_link(
    operator: &StrategyOperator,
    left: &ObservationFacts,
    right: &ObservationFacts,
) -> EntityResult<Option<EvidenceHit>> {
    match operator.op.as_str() {
        "shared_anchor" => {
            let anchor = string_param(operator, "anchor")?;
            Ok(shared_anchor_hit(
                anchor,
                left,
                right,
                EvidenceKind::MustLink,
            ))
        }
        "registry_alias_match" => Ok(registry_alias_hit(left, right, EvidenceKind::MustLink)),
        other => Err(strategy_error(
            "Unsupported must-link operator in edge engine",
            json!({ "operator": other }),
        )),
    }
}

fn evaluate_support(
    operator: &StrategyOperator,
    left: &ObservationFacts,
    right: &ObservationFacts,
) -> EntityResult<Option<EvidenceHit>> {
    match operator.op.as_str() {
        "exact_view" => {
            let view = string_param(operator, "view")?;
            let score = i64_param(operator, "score")?;
            Ok(exact_view_hit(view, score, left, right))
        }
        "acronym_plus_token" => {
            let acronym_view = string_param(operator, "acronym_view")?;
            let token_view = string_param(operator, "token_view")?;
            let score = i64_param(operator, "score")?;
            Ok(acronym_plus_token_hit(
                acronym_view,
                token_view,
                score,
                left,
                right,
            ))
        }
        "categorical_equal" => {
            let field = string_param(operator, "field")?;
            let score = i64_param(operator, "score")?;
            Ok(categorical_equal_hit(field, score, left, right))
        }
        other => Err(strategy_error(
            "Unsupported support operator in edge engine",
            json!({ "operator": other }),
        )),
    }
}

fn evaluate_cannot_link(
    operator: &StrategyOperator,
    left: &ObservationFacts,
    right: &ObservationFacts,
) -> EntityResult<Option<EvidenceHit>> {
    match operator.op.as_str() {
        "conflicting_anchor" => {
            let anchor = string_param(operator, "anchor")?;
            Ok(conflicting_anchor_hit(anchor, left, right))
        }
        other => Err(strategy_error(
            "Unsupported cannot-link operator in edge engine",
            json!({ "operator": other }),
        )),
    }
}

fn shared_anchor_hit(
    anchor: &str,
    left: &ObservationFacts,
    right: &ObservationFacts,
    kind: EvidenceKind,
) -> Option<EvidenceHit> {
    let left_values = left.anchors.get(anchor)?;
    let right_values = right.anchors.get(anchor)?;
    if left_values.is_disjoint(right_values) {
        return None;
    }

    Some(EvidenceHit {
        kind,
        namespace: ANCHOR_NAMESPACE.to_string(),
        operator_id: format!("shared_anchor:{anchor}"),
        score: 0,
        explanation: format!("{anchor} shared anchor"),
    })
}

fn conflicting_anchor_hit(
    anchor: &str,
    left: &ObservationFacts,
    right: &ObservationFacts,
) -> Option<EvidenceHit> {
    let left_values = left.anchors.get(anchor)?;
    let right_values = right.anchors.get(anchor)?;
    if left_values.is_empty() || right_values.is_empty() || !left_values.is_disjoint(right_values) {
        return None;
    }

    Some(EvidenceHit {
        kind: EvidenceKind::CannotLink,
        namespace: ANCHOR_NAMESPACE.to_string(),
        operator_id: format!("conflicting_anchor:{anchor}"),
        score: 0,
        explanation: format!("{anchor} conflicting anchor"),
    })
}

fn registry_alias_hit(
    left: &ObservationFacts,
    right: &ObservationFacts,
    kind: EvidenceKind,
) -> Option<EvidenceHit> {
    if left.registry_ids.is_disjoint(&right.registry_ids) {
        return None;
    }

    Some(EvidenceHit {
        kind,
        namespace: REGISTRY_NAMESPACE.to_string(),
        operator_id: "registry_alias_match".to_string(),
        score: 0,
        explanation: "registry alias match".to_string(),
    })
}

fn exact_view_hit(
    view: &str,
    score: i64,
    left: &ObservationFacts,
    right: &ObservationFacts,
) -> Option<EvidenceHit> {
    let left_values = left.views.get(view)?;
    let right_values = right.views.get(view)?;
    if left_values.is_disjoint(right_values) {
        return None;
    }

    Some(EvidenceHit {
        kind: EvidenceKind::Support,
        namespace: NAME_NAMESPACE.to_string(),
        operator_id: format!("exact_view:{view}"),
        score,
        explanation: format!("{view} exact match"),
    })
}

fn acronym_plus_token_hit(
    acronym_view: &str,
    token_view: &str,
    score: i64,
    left: &ObservationFacts,
    right: &ObservationFacts,
) -> Option<EvidenceHit> {
    let left_acronyms = left.views.get(acronym_view)?;
    let right_acronyms = right.views.get(acronym_view)?;
    let left_tokens = left.views.get(token_view)?;
    let right_tokens = right.views.get(token_view)?;

    let left_matches_right = left_acronyms.iter().any(|acronym| {
        right_tokens
            .iter()
            .any(|tokens| acronym == &initials_from_view(tokens))
    });
    let right_matches_left = right_acronyms.iter().any(|acronym| {
        left_tokens
            .iter()
            .any(|tokens| acronym == &initials_from_view(tokens))
    });

    if !(left_matches_right || right_matches_left) {
        return None;
    }

    Some(EvidenceHit {
        kind: EvidenceKind::Support,
        namespace: NAME_NAMESPACE.to_string(),
        operator_id: format!("acronym_plus_token:{acronym_view}:{token_view}"),
        score,
        explanation: format!("{acronym_view} acronym matches {token_view} token initials"),
    })
}

fn categorical_equal_hit(
    field: &str,
    score: i64,
    left: &ObservationFacts,
    right: &ObservationFacts,
) -> Option<EvidenceHit> {
    let left_value = left.context.get(field)?;
    let right_value = right.context.get(field)?;
    if !context_values_equal(left_value, right_value) {
        return None;
    }

    Some(EvidenceHit {
        kind: EvidenceKind::Support,
        namespace: CONTEXT_NAMESPACE.to_string(),
        operator_id: format!("categorical_equal:{field}"),
        score,
        explanation: format!("{field} categorical match"),
    })
}

fn context_values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::String(left), Value::String(right)) => {
            !left.trim().is_empty() && left.trim() == right.trim()
        }
        _ => left == right,
    }
}

fn string_param<'a>(operator: &'a StrategyOperator, key: &str) -> EntityResult<&'a str> {
    operator
        .params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            strategy_error(
                "Strategy operator is missing a required string parameter",
                json!({
                    "operator": operator.op,
                    "parameter": key,
                }),
            )
        })
}

fn i64_param(operator: &StrategyOperator, key: &str) -> EntityResult<i64> {
    operator
        .params
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            strategy_error(
                "Strategy operator is missing a required integer parameter",
                json!({
                    "operator": operator.op,
                    "parameter": key,
                }),
            )
        })
}

fn missing_row_error(field: &str, row_id: &str) -> EntityError {
    artifact_error(
        "Candidate block artifact references an unknown row",
        json!({
            "field": field,
            "row_id": row_id,
        }),
    )
}

fn strategy_error(message: &str, detail: Value) -> EntityError {
    EntityError::with_detail(EntityErrorCode::Strategy, message, detail)
}

fn artifact_error(message: &str, detail: Value) -> EntityError {
    EntityError::with_detail(EntityErrorCode::ArtifactContract, message, detail)
}

#[derive(Debug, Clone, Default)]
struct ObservationFacts {
    views: BTreeMap<String, BTreeSet<String>>,
    anchors: BTreeMap<String, BTreeSet<String>>,
    context: BTreeMap<String, Value>,
    registry_ids: BTreeSet<String>,
}

struct ObservationFactsIndex {
    strategy: EntityStrategy,
    registry_snapshot: RegistrySnapshot,
    observations: BTreeMap<String, ObservationFacts>,
}

impl ObservationFactsIndex {
    fn build(
        strategy: &EntityStrategy,
        observations: &[ProjectedObservation],
        incumbent: &IncumbentMemory,
    ) -> EntityResult<Self> {
        let alias_lookup = build_alias_lookup(incumbent);
        let mut facts_by_row = BTreeMap::new();

        for observation in observations {
            let facts = ObservationFacts {
                views: build_views(&strategy.normalize.views, observation)?,
                anchors: build_anchor_map(observation),
                context: observation.context.clone(),
                registry_ids: build_registry_ids(observation, &alias_lookup),
            };

            facts_by_row.insert(observation.source_row_id.clone(), facts);
        }

        Ok(Self {
            strategy: strategy.clone(),
            registry_snapshot: incumbent.registry.clone(),
            observations: facts_by_row,
        })
    }
}

fn build_alias_lookup(incumbent: &IncumbentMemory) -> BTreeMap<String, BTreeSet<String>> {
    let mut lookup = BTreeMap::new();
    for entry in &incumbent.alias_entries {
        let key = entry.input.trim();
        if key.is_empty() {
            continue;
        }

        lookup
            .entry(key.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(entry.canonical_id.clone());
    }
    lookup
}

fn build_views(
    view_pipelines: &BTreeMap<String, Vec<String>>,
    observation: &ProjectedObservation,
) -> EntityResult<BTreeMap<String, BTreeSet<String>>> {
    let surfaces = observation_surfaces(observation);
    let mut views = BTreeMap::new();

    for (view_name, pipeline) in view_pipelines {
        let mut values = BTreeSet::new();
        for surface in &surfaces {
            if let Some(value) = apply_view_pipeline(surface, pipeline)? {
                values.insert(value);
            }
        }
        views.insert(view_name.clone(), values);
    }

    Ok(views)
}

fn observation_surfaces(observation: &ProjectedObservation) -> Vec<String> {
    let mut surfaces = Vec::new();
    push_surface(&mut surfaces, &observation.primary_surface.value);
    for surface in &observation.alias_surfaces {
        push_surface(&mut surfaces, &surface.value);
    }
    for surface in &observation.mention_surfaces {
        push_surface(&mut surfaces, &surface.value);
    }
    surfaces
}

fn push_surface(surfaces: &mut Vec<String>, raw: &str) {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        surfaces.push(trimmed.to_string());
    }
}

fn build_anchor_map(observation: &ProjectedObservation) -> BTreeMap<String, BTreeSet<String>> {
    let mut anchors = BTreeMap::new();
    for anchor in &observation.anchors {
        let value = anchor.value.trim();
        if value.is_empty() {
            continue;
        }

        anchors
            .entry(anchor.namespace.clone())
            .or_insert_with(BTreeSet::new)
            .insert(value.to_string());
    }
    anchors
}

fn build_registry_ids(
    observation: &ProjectedObservation,
    alias_lookup: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut registry_ids = BTreeSet::new();
    for surface in observation_surfaces(observation) {
        if let Some(ids) = alias_lookup.get(surface.trim()) {
            registry_ids.extend(ids.iter().cloned());
        }
    }
    registry_ids
}

fn apply_view_pipeline(surface: &str, pipeline: &[String]) -> EntityResult<Option<String>> {
    let mut value = surface.to_string();
    for operator in pipeline {
        value = apply_normalize_operator(operator, &value)?;
    }

    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed))
    }
}

fn apply_normalize_operator(operator: &str, value: &str) -> EntityResult<String> {
    match operator {
        "lowercase" => Ok(value.to_lowercase()),
        "ascii_trim" => Ok(value
            .trim_matches(|ch: char| ch.is_ascii_whitespace())
            .to_string()),
        "strip_footnotes" => Ok(strip_trailing_footnotes(value)),
        "strip_parenthetical_notes" => Ok(strip_trailing_parenthetical_notes(value)),
        "strip_legal_suffixes" => Ok(strip_trailing_legal_suffixes(value)),
        "normalize_whitespace" => Ok(value.split_whitespace().collect::<Vec<_>>().join(" ")),
        "extract_initials" => Ok(extract_initials(value)),
        "tokenize" => Ok(split_text_tokens(value).join(" ")),
        "drop_stopwords" => Ok(split_text_tokens(value)
            .into_iter()
            .filter(|token| !STOPWORDS.contains(&token.as_str()))
            .collect::<Vec<_>>()
            .join(" ")),
        other => Err(strategy_error(
            "Unsupported normalize operator in edge engine",
            json!({ "operator": other }),
        )),
    }
}

fn split_text_tokens(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn strip_trailing_footnotes(value: &str) -> String {
    let mut output = value.trim_end().to_string();

    loop {
        let trimmed = output.trim_end();
        if !trimmed.ends_with(')') {
            break;
        }

        let Some(open_index) = trimmed.rfind('(') else {
            break;
        };
        let between = &trimmed[open_index + 1..trimmed.len() - 1];
        if between.is_empty() || !between.chars().all(|ch| ch.is_ascii_digit()) {
            break;
        }

        output = trimmed[..open_index].trim_end().to_string();
    }

    output
}

fn strip_trailing_parenthetical_notes(value: &str) -> String {
    let trimmed = value.trim_end();
    if !trimmed.ends_with(')') {
        return trimmed.to_string();
    }

    let Some(open_index) = trimmed.rfind('(') else {
        return trimmed.to_string();
    };
    trimmed[..open_index].trim_end().to_string()
}

fn strip_trailing_legal_suffixes(value: &str) -> String {
    let mut tokens = split_text_tokens(value);
    while let Some(last) = tokens.last() {
        let compound_lp = tokens.len() >= 2
            && tokens[tokens.len() - 2].as_str() == "l"
            && tokens[tokens.len() - 1].as_str() == "p";
        if compound_lp {
            tokens.pop();
            tokens.pop();
            continue;
        }

        if LEGAL_SUFFIXES.contains(&last.as_str()) {
            tokens.pop();
            continue;
        }

        break;
    }

    tokens.join(" ")
}

fn extract_initials(value: &str) -> String {
    let tokens = split_text_tokens(value);
    match tokens.as_slice() {
        [] => String::new(),
        [single] => single.clone(),
        many => many
            .iter()
            .filter_map(|token| token.chars().next())
            .collect::<String>(),
    }
}

fn initials_from_view(value: &str) -> String {
    extract_initials(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_runtime::types::{
        AliasMappingEntry, BlockHit, ProjectedAnchor, ProjectedSurface, StrategyAnchors,
        StrategyEvidence, StrategyObservations, StrategySolver,
    };

    #[test]
    fn build_edges_emits_must_link_support_and_namespace_scores() {
        let strategy = fixture_strategy();
        let registry = fixture_registry_snapshot();
        let incumbent = IncumbentMemory {
            registry: registry.clone(),
            alias_entries: vec![
                AliasMappingEntry {
                    input: "Acme Corp. (1)".to_string(),
                    canonical_id: "IC-acme".to_string(),
                    canonical_type: "issuer".to_string(),
                    rule_id: "ORG_ALIAS".to_string(),
                },
                AliasMappingEntry {
                    input: "ACME CORPORATION".to_string(),
                    canonical_id: "IC-acme".to_string(),
                    canonical_type: "issuer".to_string(),
                    rule_id: "ORG_ALIAS".to_string(),
                },
            ],
            ..IncumbentMemory::default()
        };
        let observations = vec![
            fixture_observation(
                "row-1",
                "Acme Corp. (1)",
                vec![],
                vec![("lei", "549300AAA")],
                vec![("industry", json!("Software"))],
            ),
            fixture_observation(
                "row-9",
                "ACME CORPORATION",
                vec![],
                vec![("lei", "549300AAA")],
                vec![("industry", json!("Software"))],
            ),
        ];

        let edges = build_edges(
            &strategy,
            &observations,
            &[fixture_block("row-1", "row-9", &strategy, &registry)],
            &incumbent,
        )
        .expect("edge build to succeed");

        let edge = &edges[0];
        assert!(edge.has_must_link);
        assert!(!edge.has_cannot_link);
        assert_eq!(edge.pair_score_by_namespace.get("anchor"), Some(&0));
        assert_eq!(edge.pair_score_by_namespace.get("registry"), Some(&0));
        assert_eq!(edge.pair_score_by_namespace.get("name"), Some(&32));
        assert_eq!(edge.pair_score_by_namespace.get("context"), Some(&4));
        assert_eq!(edge.pair_score_total, 36);
        assert_eq!(
            edge.hits
                .iter()
                .map(|hit| hit.operator_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "shared_anchor:lei",
                "registry_alias_match",
                "exact_view:core_name",
                "categorical_equal:industry",
            ]
        );
    }

    #[test]
    fn build_edges_caps_scores_by_namespace() {
        let strategy = fixture_strategy();
        let registry = fixture_registry_snapshot();
        let incumbent = IncumbentMemory {
            registry: registry.clone(),
            ..IncumbentMemory::default()
        };
        let observations = vec![
            fixture_observation(
                "row-1",
                "IBM",
                vec!["International Business Machines"],
                vec![],
                vec![],
            ),
            fixture_observation(
                "row-9",
                "International Business Machines",
                vec![],
                vec![],
                vec![],
            ),
        ];

        let edges = build_edges(
            &strategy,
            &observations,
            &[fixture_block("row-1", "row-9", &strategy, &registry)],
            &incumbent,
        )
        .expect("edge build to succeed");

        let edge = &edges[0];
        assert_eq!(edge.pair_score_by_namespace.get("name"), Some(&32));
        assert_eq!(edge.pair_score_total, 32);
        assert!(
            edge.hits
                .iter()
                .any(|hit| hit.operator_id == "acronym_plus_token:acronym:rare_tokens")
        );
    }

    #[test]
    fn build_edges_emits_cannot_link_for_conflicting_anchor() {
        let strategy = fixture_strategy();
        let registry = fixture_registry_snapshot();
        let incumbent = IncumbentMemory {
            registry: registry.clone(),
            ..IncumbentMemory::default()
        };
        let observations = vec![
            fixture_observation(
                "row-1",
                "Acme Corp.",
                vec![],
                vec![("lei", "549300AAA")],
                vec![],
            ),
            fixture_observation(
                "row-9",
                "Acme Corp.",
                vec![],
                vec![("lei", "549300BBB")],
                vec![],
            ),
        ];

        let edges = build_edges(
            &strategy,
            &observations,
            &[fixture_block("row-1", "row-9", &strategy, &registry)],
            &incumbent,
        )
        .expect("edge build to succeed");

        let edge = &edges[0];
        assert!(edge.has_cannot_link);
        assert!(
            edge.hits
                .iter()
                .any(|hit| hit.operator_id == "conflicting_anchor:lei")
        );
    }

    #[test]
    fn build_edges_rejects_mismatched_candidate_strategy() {
        let strategy = fixture_strategy();
        let registry = fixture_registry_snapshot();
        let incumbent = IncumbentMemory {
            registry: registry.clone(),
            ..IncumbentMemory::default()
        };
        let observations = vec![
            fixture_observation("row-1", "Acme", vec![], vec![], vec![]),
            fixture_observation("row-9", "Acme", vec![], vec![], vec![]),
        ];
        let mut candidate = fixture_block("row-1", "row-9", &strategy, &registry);
        candidate.strategy.content_hash = "blake3:other".to_string();

        let error = build_edges(&strategy, &observations, &[candidate], &incumbent)
            .expect_err("mismatched strategy to fail");

        assert_eq!(error.code, EntityErrorCode::ArtifactContract);
    }

    fn fixture_strategy() -> EntityStrategy {
        let mut anchor_fields = BTreeMap::new();
        anchor_fields.insert("lei".to_string(), "lei".to_string());

        let mut views = BTreeMap::new();
        views.insert(
            "core_name".to_string(),
            vec![
                "lowercase".to_string(),
                "strip_footnotes".to_string(),
                "strip_legal_suffixes".to_string(),
                "normalize_whitespace".to_string(),
            ],
        );
        views.insert("acronym".to_string(), vec!["extract_initials".to_string()]);
        views.insert(
            "rare_tokens".to_string(),
            vec!["tokenize".to_string(), "drop_stopwords".to_string()],
        );

        EntityStrategy {
            id: "bdc_org_graph.v1".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "issuer".to_string(),
            id_prefix: "IC".to_string(),
            observations: StrategyObservations {
                name_fields: vec!["portfolio_company".to_string()],
                required_side_fields: vec![],
                context_fields: vec!["industry".to_string()],
                anchor_fields,
            },
            normalize: super::super::types::NormalizeConfig { views },
            evidence: StrategyEvidence {
                must_link: vec![
                    strategy_operator("shared_anchor", &[("anchor", json!("lei"))]),
                    strategy_operator("registry_alias_match", &[]),
                ],
                support: vec![
                    strategy_operator(
                        "exact_view",
                        &[("view", json!("core_name")), ("score", json!(32))],
                    ),
                    strategy_operator(
                        "acronym_plus_token",
                        &[
                            ("acronym_view", json!("acronym")),
                            ("token_view", json!("rare_tokens")),
                            ("score", json!(10)),
                        ],
                    ),
                    strategy_operator(
                        "categorical_equal",
                        &[("field", json!("industry")), ("score", json!(4))],
                    ),
                ],
                cannot_link: vec![strategy_operator(
                    "conflicting_anchor",
                    &[("anchor", json!("lei"))],
                )],
            },
            anchors: StrategyAnchors {
                trusted_for_must_link: vec!["lei".to_string()],
                ..StrategyAnchors::default()
            },
            solver: StrategySolver::default(),
            content_hash: "blake3:strategy".to_string(),
            ..EntityStrategy::default()
        }
    }

    fn fixture_registry_snapshot() -> RegistrySnapshot {
        RegistrySnapshot {
            id: "bdc-issuers".to_string(),
            version: "2026.03.01".to_string(),
            source: "registries/bdc-issuers/".to_string(),
            lookup_snapshot_hash: "blake3:lookup".to_string(),
            escrow_snapshot_hash: "blake3:escrow".to_string(),
        }
    }

    fn fixture_observation(
        row_id: &str,
        primary_surface: &str,
        alias_surfaces: Vec<&str>,
        anchors: Vec<(&str, &str)>,
        context: Vec<(&str, Value)>,
    ) -> ProjectedObservation {
        ProjectedObservation {
            version: super::super::types::CANON_ENTITY_PROJECTION_VERSION.to_string(),
            source_row_id: row_id.to_string(),
            doc_id: "doc-1".to_string(),
            as_of_date: Some("2025-12-31".to_string()),
            primary_surface: ProjectedSurface {
                value: primary_surface.to_string(),
                field: "portfolio_company".to_string(),
            },
            alias_surfaces: alias_surfaces
                .into_iter()
                .map(|value| ProjectedSurface {
                    value: value.to_string(),
                    field: "alias_surfaces_json".to_string(),
                })
                .collect(),
            mention_surfaces: Vec::new(),
            anchors: anchors
                .into_iter()
                .map(|(namespace, value)| ProjectedAnchor {
                    namespace: namespace.to_string(),
                    value: value.to_string(),
                    field: namespace.to_string(),
                })
                .collect(),
            context: context
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
            provenance: BTreeMap::new(),
        }
    }

    fn fixture_block(
        left_row_id: &str,
        right_row_id: &str,
        strategy: &EntityStrategy,
        registry: &RegistrySnapshot,
    ) -> BlockRecord {
        BlockRecord {
            version: super::super::types::CANON_ENTITY_BLOCK_VERSION.to_string(),
            strategy: strategy.reference(),
            registry_snapshot: registry.clone(),
            left_row_id: left_row_id.to_string(),
            right_row_id: right_row_id.to_string(),
            block_hits: vec![BlockHit {
                operator_id: "exact_view:core_name".to_string(),
            }],
        }
    }

    fn strategy_operator(op: &str, params: &[(&str, Value)]) -> StrategyOperator {
        StrategyOperator {
            op: op.to_string(),
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }
}
